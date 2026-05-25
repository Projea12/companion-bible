//! Deepgram Nova-2 streaming transcription.
//!
//! Connects to `wss://api.deepgram.com/v1/listen`, streams 100 ms audio
//! chunks from the `SlidingWindow`, and forwards `speech_final` transcripts
//! as `TranscriptionSegment`s via the standard segment channel.
//!
//! ## Fallback
//! Call [`DeepgramTranscriber::try_connect`] before `start` to verify the
//! API key and network reachability.  If it fails, use `WhisperTranscriber`
//! instead.

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

// ─── Deepgram endpoint ────────────────────────────────────────────────────────

// All 66 Bible book names boosted so Deepgram recognises them even when
// spoken with a Nigerian accent (different vowel stress, elided syllables).
// Boost level 2 is enough to tip ambiguous phonemes without over-fitting.
const BIBLE_KEYWORDS: &[&str] = &[
    "Genesis",
    "Exodus",
    "Leviticus",
    "Numbers",
    "Deuteronomy",
    "Joshua",
    "Judges",
    "Ruth",
    "Samuel",
    "Kings",
    "Chronicles",
    "Ezra",
    "Nehemiah",
    "Esther",
    "Job",
    "Psalms",
    "Proverbs",
    "Ecclesiastes",
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
    "Matthew",
    "Mark",
    "Luke",
    "John",
    "Acts",
    "Romans",
    "Corinthians",
    "Galatians",
    "Ephesians",
    "Philippians",
    "Colossians",
    "Thessalonians",
    "Timothy",
    "Titus",
    "Philemon",
    "Hebrews",
    "James",
    "Peter",
    "Jude",
    "Revelation",
    // Common spoken forms
    "verse",
    "chapter",
    "scripture",
];

fn deepgram_url() -> String {
    // nova-2 handles diverse accents well, including Nigerian English.
    // interim_results=true + utterance_end_ms gives ~300 ms first-word latency.
    // smart_format and punctuate are intentionally disabled: both cause Deepgram
    // to break a single spoken reference ("John chapter 3 verse 16") into
    // multiple short utterances separated by periods, which prevents the
    // pattern engine from seeing the full reference in one pass.
    let keywords: String = BIBLE_KEYWORDS
        .iter()
        .map(|w| format!("&keywords={}:2", w))
        .collect();

    format!(
        "wss://api.deepgram.com/v1/listen\
         ?model=nova-2\
         &language=en\
         &interim_results=true\
         &utterance_end_ms=1000\
         &encoding=linear16\
         &sample_rate=16000\
         &channels=1\
         {keywords}"
    )
}

// ─── Wire types ───────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct DgAlternative {
    transcript: String,
    confidence: Option<f32>,
}

#[derive(Deserialize)]
struct DgChannel {
    alternatives: Vec<DgAlternative>,
}

#[derive(Deserialize)]
struct DgResult {
    channel: Option<DgChannel>,
    is_final: Option<bool>,
    speech_final: Option<bool>,
    start: Option<f64>,
    duration: Option<f64>,
}

// ─── DeepgramTranscriber ─────────────────────────────────────────────────────

pub struct DeepgramTranscriber {
    api_key: String,
    window: Arc<Mutex<SlidingWindow>>,
    stop_flag: Arc<AtomicBool>,
    handle: Option<tokio::task::JoinHandle<()>>,
    sender: SegmentSender,
}

impl DeepgramTranscriber {
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

    /// Attempt a WebSocket handshake to verify the API key and connectivity.
    /// Returns `Ok(())` on success, `Err(reason)` otherwise.
    pub async fn try_connect(api_key: &str) -> Result<(), String> {
        use tokio_tungstenite::tungstenite::client::IntoClientRequest;
        let mut req = deepgram_url()
            .into_client_request()
            .map_err(|e| e.to_string())?;
        req.headers_mut().insert(
            "Authorization",
            format!("Token {api_key}").parse().map_err(
                |e: tokio_tungstenite::tungstenite::http::header::InvalidHeaderValue| e.to_string(),
            )?,
        );
        tokio_tungstenite::connect_async(req)
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    /// Start the streaming task.
    pub fn start(&mut self) {
        self.stop_flag.store(false, Ordering::Release);
        let api_key = self.api_key.clone();
        let window = Arc::clone(&self.window);
        let stop_flag = Arc::clone(&self.stop_flag);
        let sender = self.sender.clone();

        self.handle = Some(tokio::spawn(async move {
            let mut backoff_secs = 1u64;
            loop {
                if stop_flag.load(Ordering::Acquire) {
                    break;
                }

                let connected_at = tokio::time::Instant::now();
                match stream_loop(
                    api_key.clone(),
                    Arc::clone(&window),
                    Arc::clone(&stop_flag),
                    sender.clone(),
                )
                .await
                {
                    Ok(()) => {
                        if stop_flag.load(Ordering::Acquire) {
                            break; // deliberate stop — do not reconnect
                        }
                        eprintln!("[deepgram] stream closed — reconnecting");
                    }
                    Err(e) => {
                        if stop_flag.load(Ordering::Acquire) {
                            break;
                        }
                        eprintln!("[deepgram] stream error: {e} — reconnecting in {backoff_secs}s");
                    }
                }

                // Reset backoff if the session lasted more than 10 s (healthy run).
                if connected_at.elapsed().as_secs() > 10 {
                    backoff_secs = 1;
                }

                tokio::time::sleep(tokio::time::Duration::from_secs(backoff_secs)).await;
                backoff_secs = (backoff_secs * 2).min(30);
            }
            eprintln!("[deepgram] supervisor stopped");
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

impl Drop for DeepgramTranscriber {
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

    let mut req = deepgram_url().into_client_request()?;
    req.headers_mut()
        .insert("Authorization", format!("Token {api_key}").parse()?);

    let (ws_stream, _) = tokio_tungstenite::connect_async(req).await?;
    let (mut write, mut read) = ws_stream.split();
    eprintln!("[deepgram] connected — streaming audio");

    // Audio sender: drain the SlidingWindow every 100 ms and send i16 PCM.
    let stop_audio = Arc::clone(&stop_flag);
    let mut audio_handle = tokio::spawn(async move {
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
        // Signal end-of-stream to Deepgram.
        let _ = write
            .send(Message::Text(r#"{"type":"CloseStream"}"#.into()))
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
                eprintln!("[deepgram] server closed connection: {:?}", frame);
                break;
            }
            other => {
                eprintln!("[deepgram] non-text message: {:?}", other);
                continue;
            }
        };

        eprintln!("[deepgram] raw msg: {text}");

        let result: DgResult = match serde_json::from_str(&text) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[deepgram] parse error: {e}");
                continue;
            }
        };

        // Accept speech_final (end of utterance) or is_final (end of chunk).
        if !result.speech_final.unwrap_or(false) && !result.is_final.unwrap_or(false) {
            continue;
        }

        let channel = match result.channel {
            Some(c) => c,
            None => continue,
        };
        let alt = match channel.alternatives.into_iter().next() {
            Some(a) => a,
            None => continue,
        };

        let transcript = alt.transcript.trim().to_string();
        if transcript.is_empty() {
            continue;
        }

        let start_ms = (result.start.unwrap_or(0.0) * 1_000.0) as u64;
        let dur_ms = (result.duration.unwrap_or(0.0) * 1_000.0) as u64;

        let mut seg = TranscriptionSegment {
            text: transcript,
            audio_start_ms: start_ms,
            audio_end_ms: start_ms + dur_ms,
            whisper_confidence: alt.confidence.unwrap_or(1.0),
            is_duplicate: false,
            context_window: String::new(),
        };
        correct_segment(&mut seg);
        sender.send(vec![seg]);
    }

    // Wait up to 2 s for the audio task to send its CloseStream frame before
    // forcibly cancelling it. The task exits within one 100 ms tick after
    // stop_flag is set; the timeout is only reached on unexpected server close.
    match tokio::time::timeout(tokio::time::Duration::from_secs(2), &mut audio_handle).await {
        Ok(_) => {}
        Err(_) => audio_handle.abort(),
    }
    Ok(())
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    use std::time::Duration;

    // A task that finishes quickly must complete before the 2 s timeout fires,
    // i.e. we never reach the abort() branch.
    #[tokio::test]
    async fn fast_audio_task_completes_without_abort() {
        let mut handle = tokio::spawn(async {
            tokio::time::sleep(Duration::from_millis(10)).await;
        });
        let result = tokio::time::timeout(Duration::from_secs(2), &mut handle).await;
        assert!(
            result.is_ok(),
            "fast task should finish before the 2 s deadline"
        );
    }

    // A task that never finishes must be aborted after the timeout, and the
    // JoinHandle must reflect a cancellation error — not silently detach.
    #[tokio::test]
    async fn hung_audio_task_is_aborted_after_timeout() {
        let mut handle = tokio::spawn(async {
            tokio::time::sleep(Duration::from_secs(100)).await;
        });
        match tokio::time::timeout(Duration::from_millis(50), &mut handle).await {
            Ok(_) => panic!("should have timed out"),
            Err(_) => handle.abort(),
        }
        let join_result = handle.await;
        assert!(
            join_result.is_err() && join_result.unwrap_err().is_cancelled(),
            "aborted task must produce a cancellation JoinError"
        );
    }

    // The audio loop checks stop_flag every 100 ms tick. After the flag is set
    // the task must exit — and therefore allow a clean await — well within
    // the 2 s timeout window used in stream_loop.
    #[tokio::test]
    async fn stop_flag_exits_audio_loop_within_timeout_window() {
        let stop_flag = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop_flag);

        let mut handle = tokio::spawn(async move {
            let tick = tokio::time::Duration::from_millis(100);
            loop {
                if flag.load(Ordering::Acquire) {
                    break;
                }
                tokio::time::sleep(tick).await;
            }
            // Simulate the CloseStream send (fast I/O, ≤10 ms in practice).
            tokio::time::sleep(Duration::from_millis(10)).await;
        });

        // Set the flag mid-tick.
        tokio::time::sleep(Duration::from_millis(50)).await;
        stop_flag.store(true, Ordering::Release);

        // Must complete long before the 2 s production timeout.
        let result = tokio::time::timeout(Duration::from_secs(2), &mut handle).await;
        assert!(
            result.is_ok(),
            "audio task must exit within 2 s after stop_flag is set"
        );
    }
}
