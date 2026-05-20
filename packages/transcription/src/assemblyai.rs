//! AssemblyAI real-time streaming transcription.
//!
//! Connects to `wss://api.assemblyai.com/v2/realtime/ws`, streams 100 ms audio
//! chunks from the `SlidingWindow`, and forwards `FinalTranscript` messages as
//! `TranscriptionSegment`s via the standard segment channel.
//!
//! ## Why AssemblyAI
//!
//! AssemblyAI's Nano model has broader accent training data than Deepgram Nova-2,
//! including African English variants commonly spoken by Nigerian pastors.
//! The `word_boost` + `boost_param=high` parameters further bias the decoder
//! toward all 66 Bible book names.
//!
//! ## Fallback
//! Call [`AssemblyAiTranscriber::try_connect`] before `start` to verify the API
//! key and network reachability.  If it fails, fall back to `DeepgramTranscriber`.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

use companion_audio::SlidingWindow;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio_tungstenite::tungstenite::Message;

use crate::channel::{segment_channel, SegmentReceiver, SegmentSender};
use crate::correction::correct_segment;
use crate::transcript::TranscriptionSegment;

// ─── Bible book list for word_boost ──────────────────────────────────────────

// All 66 Bible book names boosted so AssemblyAI recognises them even when
// spoken with a Nigerian accent.  boost_param=high gives maximum weight.
const BOOST_WORDS: &[&str] = &[
    "Genesis", "Exodus", "Leviticus", "Numbers", "Deuteronomy",
    "Joshua", "Judges", "Ruth", "Samuel", "Kings", "Chronicles",
    "Ezra", "Nehemiah", "Esther", "Job", "Psalms", "Proverbs",
    "Ecclesiastes", "Isaiah", "Jeremiah", "Lamentations", "Ezekiel",
    "Daniel", "Hosea", "Joel", "Amos", "Obadiah", "Jonah", "Micah",
    "Nahum", "Habakkuk", "Zephaniah", "Haggai", "Zechariah", "Malachi",
    "Matthew", "Mark", "Luke", "John", "Acts", "Romans",
    "Corinthians", "Galatians", "Ephesians", "Philippians", "Colossians",
    "Thessalonians", "Timothy", "Titus", "Philemon", "Hebrews", "James",
    "Peter", "Jude", "Revelation",
    "verse", "chapter", "scripture",
];

fn assemblyai_url() -> String {
    // AssemblyAI real-time API expects word_boost as a percent-encoded JSON array:
    // &word_boost=%5B%22Genesis%22%2C%22Exodus%22%2C...%5D
    // smart_format intentionally disabled — same reason as Deepgram: it breaks
    // references like "John chapter 3 verse 16" into multiple short utterances.
    let words: Vec<String> = BOOST_WORDS.iter().map(|w| format!("\"{}\"", w)).collect();
    let json_arr = format!("[{}]", words.join(","));
    let encoded: String = json_arr
        .chars()
        .map(|c| match c {
            '[' => "%5B".to_string(),
            ']' => "%5D".to_string(),
            '"' => "%22".to_string(),
            ',' => "%2C".to_string(),
            ' ' => "%20".to_string(),
            c => c.to_string(),
        })
        .collect();
    format!(
        "wss://api.assemblyai.com/v2/realtime/ws\
         ?sample_rate=16000\
         &word_boost={encoded}\
         &boost_param=high"
    )
}

// ─── Wire types ───────────────────────────────────────────────────────────────

#[derive(Deserialize, Debug)]
struct AaiMessage {
    message_type: String,
    #[serde(default)]
    text: String,
    confidence: Option<f64>,
    audio_start: Option<u64>,
    audio_end: Option<u64>,
}

// ─── AssemblyAiTranscriber ────────────────────────────────────────────────────

pub struct AssemblyAiTranscriber {
    api_key: String,
    window: Arc<Mutex<SlidingWindow>>,
    stop_flag: Arc<AtomicBool>,
    handle: Option<tokio::task::JoinHandle<()>>,
    sender: SegmentSender,
}

impl AssemblyAiTranscriber {
    pub fn new(api_key: String, window: Arc<Mutex<SlidingWindow>>) -> (Self, SegmentReceiver) {
        let (sender, receiver) = segment_channel();
        let stop_flag = Arc::new(AtomicBool::new(true));
        (Self { api_key, window, stop_flag, handle: None, sender }, receiver)
    }

    /// Attempt a WebSocket handshake to verify the API key and connectivity.
    ///
    /// Waits for `SessionBegins` to confirm authentication succeeded.
    /// Returns `Ok(())` on success, `Err(reason)` otherwise.
    pub async fn try_connect(api_key: &str) -> Result<(), String> {
        use tokio_tungstenite::tungstenite::client::IntoClientRequest;

        let mut req = assemblyai_url()
            .into_client_request()
            .map_err(|e| e.to_string())?;
        req.headers_mut().insert(
            "Authorization",
            api_key
                .parse()
                .map_err(|e: tokio_tungstenite::tungstenite::http::header::InvalidHeaderValue| {
                    e.to_string()
                })?,
        );
        let (mut ws, _) = tokio_tungstenite::connect_async(req)
            .await
            .map_err(|e| e.to_string())?;

        // Read messages until SessionBegins (auth OK) or error.
        while let Some(msg) = ws.next().await {
            let text = match msg.map_err(|e| e.to_string())? {
                Message::Text(t) => t,
                _ => continue,
            };
            let val: serde_json::Value =
                serde_json::from_str(&text).map_err(|e| e.to_string())?;
            match val["message_type"].as_str() {
                Some("SessionBegins") => return Ok(()),
                Some("Error") => {
                    return Err(
                        val["error"].as_str().unwrap_or("unknown error").to_string()
                    )
                }
                _ => {}
            }
        }
        Err("connection closed without SessionBegins".to_string())
    }

    /// Start the streaming task.
    pub fn start(&mut self) {
        self.stop_flag.store(false, Ordering::Release);
        let api_key = self.api_key.clone();
        let window = Arc::clone(&self.window);
        let stop_flag = Arc::clone(&self.stop_flag);
        let sender = self.sender.clone();

        self.handle = Some(tokio::spawn(async move {
            match stream_loop(api_key, window, stop_flag, sender).await {
                Ok(()) => eprintln!("[assemblyai] stream closed"),
                Err(e) => eprintln!("[assemblyai] stream error: {e}"),
            }
        }));
    }

    pub fn stop(&mut self) {
        self.stop_flag.store(true, Ordering::Release);
        if let Some(h) = self.handle.take() {
            h.abort();
        }
    }

    pub fn is_running(&self) -> bool {
        self.handle.is_some()
    }
}

impl Drop for AssemblyAiTranscriber {
    fn drop(&mut self) {
        self.stop();
    }
}

// ─── Stream loop ─────────────────────────────────────────────────────────────

async fn stream_loop(
    api_key: String,
    window: Arc<Mutex<SlidingWindow>>,
    stop_flag: Arc<AtomicBool>,
    sender: SegmentSender,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;

    let mut req = assemblyai_url().into_client_request()?;
    req.headers_mut().insert("Authorization", api_key.parse()?);

    let (ws_stream, _) = tokio_tungstenite::connect_async(req).await?;
    let (mut write, mut read) = ws_stream.split();
    eprintln!("[assemblyai] connected — streaming audio");

    // Audio sender: drain the SlidingWindow every 100 ms and send i16 PCM.
    let stop_audio = Arc::clone(&stop_flag);
    let audio_handle = tokio::spawn(async move {
        let tick = tokio::time::Duration::from_millis(100);
        loop {
            if stop_audio.load(Ordering::Acquire) {
                break;
            }
            tokio::time::sleep(tick).await;

            let samples: Vec<f32> = {
                match window.lock() {
                    Ok(mut w) => w.drain_all(),
                    Err(_) => break,
                }
            };

            if samples.is_empty() {
                continue;
            }

            let bytes: Vec<u8> = samples
                .iter()
                .flat_map(|&s| {
                    let i = (s * 32_767.0).clamp(-32_768.0, 32_767.0) as i16;
                    i.to_le_bytes()
                })
                .collect();

            if write.send(Message::Binary(bytes)).await.is_err() {
                break;
            }
        }
        // Graceful session termination.
        let _ = write
            .send(Message::Text(r#"{"terminate_session": true}"#.into()))
            .await;
    });

    // Receive transcription results.
    while let Some(msg) = read.next().await {
        if stop_flag.load(Ordering::Acquire) {
            break;
        }
        let text = match msg? {
            Message::Text(t) => t,
            Message::Close(frame) => {
                eprintln!("[assemblyai] server closed connection: {:?}", frame);
                break;
            }
            other => {
                eprintln!("[assemblyai] non-text message: {:?}", other);
                continue;
            }
        };

        eprintln!("[assemblyai] raw msg: {text}");

        let result: AaiMessage = match serde_json::from_str(&text) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[assemblyai] parse error: {e}");
                continue;
            }
        };

        // Only process FinalTranscript — PartialTranscript would flood the detection engine.
        if result.message_type != "FinalTranscript" {
            continue;
        }

        let transcript = result.text.trim().to_string();
        if transcript.is_empty() {
            continue;
        }

        let start_ms = result.audio_start.unwrap_or(0);
        let end_ms = result.audio_end.unwrap_or(start_ms);

        let mut seg = TranscriptionSegment {
            text: transcript,
            audio_start_ms: start_ms,
            audio_end_ms: end_ms,
            whisper_confidence: result.confidence.unwrap_or(1.0) as f32,
            is_duplicate: false,
            context_window: String::new(),
        };
        correct_segment(&mut seg);
        sender.send(vec![seg]);
    }

    audio_handle.abort();
    Ok(())
}
