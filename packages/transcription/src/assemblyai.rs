//! AssemblyAI Universal-3 Pro streaming transcription (v3 API).
//!
//! Connects to `wss://streaming.assemblyai.com/v3/ws` using the `u3-rt-pro`
//! model — AssemblyAI's highest-accuracy real-time model with native multilingual
//! support including African English variants and Nigerian accents.
//!
//! ## Protocol (v3 differs substantially from v2)
//!
//! 1. Connect with `Authorization` header + URL params.
//! 2. Wait for `{"type":"Begin"}` from the server.
//! 3. Send `UpdateConfiguration` to push keyterms and turn-detection tuning.
//! 4. Stream raw i16 PCM binary frames continuously.
//! 5. Process `Turn` messages where `end_of_turn == true` (final transcripts).
//! 6. Terminate with `{"type":"Terminate"}` when done.
//!
//! ## Why u3-rt-pro over the legacy Nano model
//!
//! * Up to 21% better accuracy overall; significantly better on accented English.
//! * Keyterms prompting (sent via UpdateConfiguration) biases the decoder toward
//!   all 66 Bible book names at connection time — not just URL-encoded hints.
//! * Punctuation-based turn detection produces more coherent verse references
//!   because the model knows when a sentence is grammatically complete.

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

// ─── Keyterms (max 100 per session) ──────────────────────────────────────────

/// All 66 Bible book names plus numbered variants and common terms.
/// Sent to AssemblyAI via UpdateConfiguration after Begin — up to 100 allowed.
const KEYTERMS: &[&str] = &[
    // Old Testament
    "Genesis",
    "Exodus",
    "Leviticus",
    "Numbers",
    "Deuteronomy",
    "Joshua",
    "Judges",
    "Ruth",
    "Samuel",
    "First Samuel",
    "Second Samuel",
    "Kings",
    "First Kings",
    "Second Kings",
    "Chronicles",
    "First Chronicles",
    "Second Chronicles",
    "Ezra",
    "Nehemiah",
    "Esther",
    "Job",
    "Psalm",
    "Psalms",
    "Proverbs",
    "Ecclesiastes",
    "Song of Solomon",
    "Song of Songs",
    "Isaiah",
    "Jeremiah",
    "Lamentations",
    "Ezekiel",
    "Daniel",
    "Hosea",
    "Joel",
    "Amos",
    "Obadiah",
    "Jonah",
    "Micah",
    "Nahum",
    "Habakkuk",
    "Zephaniah",
    "Haggai",
    "Zechariah",
    "Malachi",
    // New Testament
    "Matthew",
    "Mark",
    "Luke",
    "John",
    "Acts",
    "Romans",
    "Corinthians",
    "First Corinthians",
    "Second Corinthians",
    "Galatians",
    "Ephesians",
    "Philippians",
    "Colossians",
    "Thessalonians",
    "First Thessalonians",
    "Second Thessalonians",
    "Timothy",
    "First Timothy",
    "Second Timothy",
    "Titus",
    "Philemon",
    "Hebrews",
    "James",
    "Peter",
    "First Peter",
    "Second Peter",
    "First John",
    "Second John",
    "Third John",
    "Jude",
    "Revelation",
    // Common scripture terms
    "verse",
    "chapter",
    "scripture",
    "passage",
];

const API_ENDPOINT: &str = "wss://streaming.assemblyai.com/v3/ws\
     ?sample_rate=16000\
     &speech_model=u3-rt-pro";

// ─── Wire types ───────────────────────────────────────────────────────────────

#[derive(Deserialize, Debug)]
struct AaiMessage {
    #[serde(rename = "type")]
    msg_type: String,
    /// Populated on Turn messages.
    #[serde(default)]
    transcript: String,
    /// true = end-of-turn (final, formatted); false = partial (in-progress).
    #[serde(default)]
    #[allow(dead_code)]
    end_of_turn: bool,
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
        (
            Self {
                api_key,
                window,
                stop_flag,
                handle: None,
                sender,
            },
            receiver,
        )
    }

    /// Verify the API key and network reachability.
    ///
    /// Connects, waits for `Begin` (auth confirmed), then closes.
    pub async fn try_connect(api_key: &str) -> Result<(), String> {
        use tokio_tungstenite::tungstenite::client::IntoClientRequest;

        let mut req = API_ENDPOINT
            .into_client_request()
            .map_err(|e| e.to_string())?;
        req.headers_mut().insert(
            "Authorization",
            api_key.parse().map_err(
                |e: tokio_tungstenite::tungstenite::http::header::InvalidHeaderValue| e.to_string(),
            )?,
        );

        let (mut ws, _) = tokio_tungstenite::connect_async(req)
            .await
            .map_err(|e| e.to_string())?;

        while let Some(msg) = ws.next().await {
            let text = match msg.map_err(|e| e.to_string())? {
                Message::Text(t) => t,
                _ => continue,
            };
            let val: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
            match val["type"].as_str() {
                Some("Begin") => {
                    let _ = ws
                        .send(Message::Text(r#"{"type":"Terminate"}"#.into()))
                        .await;
                    return Ok(());
                }
                Some("Error") => {
                    return Err(val["error"].as_str().unwrap_or("unknown error").to_string())
                }
                _ => {}
            }
        }
        Err("connection closed without Begin".to_string())
    }

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

// ─── Session configuration ────────────────────────────────────────────────────

fn build_update_configuration() -> String {
    let terms: Vec<String> = KEYTERMS.iter().map(|k| format!("\"{}\"", k)).collect();
    let arr = format!("[{}]", terms.join(","));
    // min_turn_silence 200ms: check for end-of-turn after 200ms of silence.
    // max_turn_silence 2500ms: force end-of-turn after 2.5s of silence — generous
    // enough for a pastor who pauses between "Romans" and "chapter 8 verse 28".
    // prompt: steer toward English + verbatim transcription of spoken references.
    serde_json::json!({
        "type": "UpdateConfiguration",
        "keyterms_prompt": serde_json::from_str::<serde_json::Value>(&arr).unwrap_or(serde_json::Value::Array(vec![])),
        "min_turn_silence": 200,
        "max_turn_silence": 2500,
        "prompt": "Transcribe English. Transcribe verbatim with standard punctuation. \
                   Include filler words and incomplete utterances."
    })
    .to_string()
}

// ─── Stream loop ─────────────────────────────────────────────────────────────

async fn stream_loop(
    api_key: String,
    window: Arc<Mutex<SlidingWindow>>,
    stop_flag: Arc<AtomicBool>,
    sender: SegmentSender,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;

    let mut req = API_ENDPOINT.into_client_request()?;
    req.headers_mut().insert("Authorization", api_key.parse()?);

    let (ws_stream, _) = tokio_tungstenite::connect_async(req).await?;
    let (mut write, mut read) = ws_stream.split();
    eprintln!("[assemblyai] connected, waiting for Begin...");

    // ── Wait for Begin, then configure ───────────────────────────────────────
    loop {
        match read.next().await {
            None => return Err("connection closed before Begin".into()),
            Some(Err(e)) => return Err(e.into()),
            Some(Ok(Message::Text(text))) => {
                let val: serde_json::Value = serde_json::from_str(&text)?;
                match val["type"].as_str() {
                    Some("Begin") => {
                        eprintln!("[assemblyai] session started — sending configuration");
                        write
                            .send(Message::Text(build_update_configuration()))
                            .await?;
                        break;
                    }
                    Some("Error") => {
                        return Err(val["error"]
                            .as_str()
                            .unwrap_or("unknown error")
                            .to_string()
                            .into())
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    // Discard audio that accumulated in the window during the WebSocket
    // handshake (connect + Begin + UpdateConfiguration can take 300 ms – 2 s
    // depending on network).  Sending it all at once in the first binary frame
    // triggers AssemblyAI error 3007 (Input Duration > 1000 ms).
    if let Ok(mut w) = window.lock() {
        let discarded = w.drain_all().len();
        if discarded > 0 {
            eprintln!("[assemblyai] discarded {discarded} stale samples from connection backlog");
        }
    }

    // Max samples per message: 800 ms at 16 kHz.
    // Slicing keeps us safely under AssemblyAI's 1000 ms per-frame limit even
    // if drain_all() returns slightly more than expected.
    const MAX_SAMPLES: usize = 16_000 * 800 / 1000; // 12 800

    // ── Audio sender task ─────────────────────────────────────────────────────
    let stop_audio = Arc::clone(&stop_flag);
    let audio_handle = tokio::spawn(async move {
        let tick = tokio::time::Duration::from_millis(100);
        loop {
            if stop_audio.load(Ordering::Acquire) {
                break;
            }
            tokio::time::sleep(tick).await;

            let samples: Vec<f32> = match window.lock() {
                Ok(mut w) => w.drain_all(),
                Err(_) => break,
            };

            if samples.is_empty() {
                continue;
            }

            // Send in ≤800 ms slices so we never exceed the 1000 ms limit.
            for slice in samples.chunks(MAX_SAMPLES) {
                let bytes: Vec<u8> = slice
                    .iter()
                    .flat_map(|&s| {
                        let i = (s * 32_767.0).clamp(-32_768.0, 32_767.0) as i16;
                        i.to_le_bytes()
                    })
                    .collect();
                if write.send(Message::Binary(bytes)).await.is_err() {
                    return;
                }
            }
        }
        // Graceful session termination.
        let _ = write
            .send(Message::Text(r#"{"type":"Terminate"}"#.into()))
            .await;
    });

    // ── Receive transcript results ────────────────────────────────────────────
    while let Some(msg) = read.next().await {
        if stop_flag.load(Ordering::Acquire) {
            break;
        }
        let text = match msg? {
            Message::Text(t) => t,
            Message::Close(frame) => {
                eprintln!("[assemblyai] server closed: {:?}", frame);
                break;
            }
            _ => continue,
        };

        eprintln!("[assemblyai] msg: {text}");

        let result: AaiMessage = match serde_json::from_str(&text) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[assemblyai] parse error: {e} — raw: {text}");
                continue;
            }
        };

        if result.msg_type != "Turn" {
            continue;
        }

        let transcript = result.transcript.trim().to_string();
        if transcript.is_empty() {
            continue;
        }

        let mut seg = TranscriptionSegment {
            text: transcript,
            audio_start_ms: 0,
            audio_end_ms: 0,
            whisper_confidence: 1.0,
            is_duplicate: false,
            context_window: String::new(),
        };
        correct_segment(&mut seg);
        sender.send(vec![seg]);
    }

    audio_handle.abort();
    Ok(())
}
