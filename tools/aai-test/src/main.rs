//! Standalone AssemblyAI streaming test.
//!
//! Usage:
//!   ASSEMBLYAI_KEY=aai-... cargo run --manifest-path tools/aai-test/Cargo.toml
//!
//! What it does:
//!   1. Opens your default microphone at native rate (48 kHz on M1 Mac).
//!   2. Downsamples to 16 kHz using box-filter averaging (no other processing).
//!   3. Connects to AssemblyAI v3 (u3-rt-pro model) via WebSocket.
//!   4. Streams audio and prints every message received from AssemblyAI.
//!
//! Every raw WebSocket message is printed so you can see exactly what
//! AssemblyAI is returning — or diagnose why it is not returning anything.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::SampleFormat;
use futures_util::{SinkExt, StreamExt};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

const TARGET_RATE: u32 = 16_000;
const ENDPOINT: &str =
    "wss://streaming.assemblyai.com/v3/ws\
     ?sample_rate=16000\
     &speech_model=u3-rt-pro";

#[tokio::main]
async fn main() {
    let api_key = std::env::var("ASSEMBLYAI_KEY").unwrap_or_else(|_| {
        eprintln!("\nUsage:  ASSEMBLYAI_KEY=aai-... cargo run --manifest-path tools/aai-test/Cargo.toml\n");
        std::process::exit(1);
    });

    // ── 1. Open microphone ────────────────────────────────────────────────────

    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .expect("No input device found — check microphone permissions");
    let device_name = device.name().unwrap_or_else(|_| "unknown".into());

    let config = device
        .default_input_config()
        .expect("Could not get default input config");
    let native_rate = config.sample_rate().0;
    let channels = config.channels() as usize;
    let fmt = config.sample_format();

    println!("═══════════════════════════════════════════════════");
    println!(" AssemblyAI standalone transcription test");
    println!("═══════════════════════════════════════════════════");
    println!(" Microphone : {device_name}");
    println!(" Native rate: {native_rate} Hz  channels: {channels}  format: {fmt:?}");
    println!(" Target rate: {TARGET_RATE} Hz (downsample ratio {})", native_rate / TARGET_RATE);
    println!(" Model      : u3-rt-pro  (Universal-3 Pro Streaming)");
    println!("═══════════════════════════════════════════════════");
    println!(" Speak into your microphone. Press Ctrl+C to stop.");
    println!("═══════════════════════════════════════════════════\n");

    let (audio_tx, mut audio_rx) = mpsc::unbounded_channel::<Vec<f32>>();
    let stop = Arc::new(AtomicBool::new(false));

    // Build CPAL stream — no noise reduction, no gate, just mono + downsample.
    let stop_stream = Arc::clone(&stop);
    let err_fn = |e: cpal::StreamError| eprintln!("[cpal] stream error: {e}");

    let stream = match fmt {
        SampleFormat::F32 => device.build_input_stream(
            &config.into(),
            {
                let tx = audio_tx.clone();
                move |data: &[f32], _| {
                    if stop_stream.load(Ordering::Acquire) { return; }
                    let mono = to_mono_f32(data, channels);
                    let resampled = downsample_box(&mono, native_rate, TARGET_RATE);
                    let peak = resampled.iter().copied().fold(0.0f32, f32::max);
                    // Print audio level so we can confirm audio is flowing.
                    eprint!("\r[audio] level: {:.4}  chunks: {} samples     ",
                        peak, resampled.len());
                    let _ = tx.send(resampled);
                }
            },
            err_fn,
            None,
        ),
        SampleFormat::I16 => device.build_input_stream(
            &config.into(),
            {
                let tx = audio_tx.clone();
                move |data: &[i16], _| {
                    if stop_stream.load(Ordering::Acquire) { return; }
                    let f32s: Vec<f32> = data.iter().map(|&s| s as f32 / i16::MAX as f32).collect();
                    let mono = to_mono_f32(&f32s, channels);
                    let resampled = downsample_box(&mono, native_rate, TARGET_RATE);
                    let peak = resampled.iter().copied().fold(0.0f32, f32::max);
                    eprint!("\r[audio] level: {:.4}  chunks: {} samples     ",
                        peak, resampled.len());
                    let _ = tx.send(resampled);
                }
            },
            err_fn,
            None,
        ),
        SampleFormat::U16 => device.build_input_stream(
            &config.into(),
            {
                let tx = audio_tx.clone();
                move |data: &[u16], _| {
                    if stop_stream.load(Ordering::Acquire) { return; }
                    let f32s: Vec<f32> = data.iter()
                        .map(|&s| (s as f32 / u16::MAX as f32) * 2.0 - 1.0)
                        .collect();
                    let mono = to_mono_f32(&f32s, channels);
                    let resampled = downsample_box(&mono, native_rate, TARGET_RATE);
                    let peak = resampled.iter().copied().fold(0.0f32, f32::max);
                    eprint!("\r[audio] level: {:.4}  chunks: {} samples     ",
                        peak, resampled.len());
                    let _ = tx.send(resampled);
                }
            },
            err_fn,
            None,
        ),
        other => panic!("Unsupported sample format: {other:?}"),
    }
    .expect("Failed to build input stream");

    stream.play().expect("Failed to start audio stream");

    // ── 2. Connect to AssemblyAI ──────────────────────────────────────────────

    println!("[ws] connecting to AssemblyAI...");

    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    let mut req = ENDPOINT.into_client_request().expect("bad URL");
    req.headers_mut().insert(
        "Authorization",
        api_key.parse().expect("bad api key format"),
    );

    let (ws, _) = tokio_tungstenite::connect_async(req)
        .await
        .expect("WebSocket connection failed — check your API key and internet");

    println!("[ws] connected!\n");

    let (mut write, mut read) = ws.split();

    // ── 3. Wait for Begin, then send config ───────────────────────────────────

    loop {
        match read.next().await {
            None => panic!("[ws] connection closed before Begin"),
            Some(Err(e)) => panic!("[ws] error before Begin: {e}"),
            Some(Ok(Message::Text(text))) => {
                println!("[ws ←] {text}");
                let val: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
                match val["type"].as_str() {
                    Some("Begin") => {
                        println!("[ws] session started — sending configuration\n");
                        let config_msg = serde_json::json!({
                            "type": "UpdateConfiguration",
                            "keyterms_prompt": [
                                "Genesis","Exodus","Leviticus","Numbers","Deuteronomy",
                                "Joshua","Judges","Ruth","Samuel","Kings","Chronicles",
                                "Ezra","Nehemiah","Esther","Job","Psalm","Psalms","Proverbs",
                                "Ecclesiastes","Song of Solomon","Isaiah","Jeremiah",
                                "Lamentations","Ezekiel","Daniel","Hosea","Joel","Amos",
                                "Obadiah","Jonah","Micah","Nahum","Habakkuk","Zephaniah",
                                "Haggai","Zechariah","Malachi","Matthew","Mark","Luke",
                                "John","Acts","Romans","Corinthians","Galatians","Ephesians",
                                "Philippians","Colossians","Thessalonians","Timothy","Titus",
                                "Philemon","Hebrews","James","Peter","Jude","Revelation",
                                "verse","chapter","scripture","passage"
                            ],
                            "min_turn_silence": 200,
                            "max_turn_silence": 2500,
                            "prompt": "Transcribe English. Transcribe verbatim with standard punctuation. Include filler words and incomplete utterances."
                        }).to_string();
                        write.send(Message::Text(config_msg)).await
                            .expect("Failed to send config");
                        break;
                    }
                    Some("Error") => {
                        panic!(
                            "[ws] AUTH ERROR: {}",
                            val["error"].as_str().unwrap_or("unknown")
                        );
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    // ── 4. Stream audio + receive transcripts ─────────────────────────────────

    // Discard audio that piled up during the WebSocket handshake.
    // Without this the first message would be several seconds long and
    // AssemblyAI rejects anything over 1000 ms.
    let mut discarded = 0usize;
    while let Ok(chunk) = audio_rx.try_recv() {
        discarded += chunk.len();
    }
    eprintln!("\n[audio] discarded {discarded} stale samples from handshake backlog");

    // Max samples per message: 800 ms at 16 kHz = 12 800 samples.
    const MAX_SAMPLES: usize = TARGET_RATE as usize * 800 / 1000;

    let stop_sender = Arc::clone(&stop);
    let sender_handle = tokio::spawn(async move {
        let mut audio_rx = audio_rx;
        let mut batch: Vec<f32> = Vec::new();
        let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(100));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if stop_sender.load(Ordering::Acquire) { break; }
                    while let Ok(chunk) = audio_rx.try_recv() {
                        batch.extend_from_slice(&chunk);
                    }
                    if batch.is_empty() { continue; }

                    // Send in ≤800 ms slices so we never exceed the 1000 ms limit.
                    for slice in batch.chunks(MAX_SAMPLES) {
                        let bytes: Vec<u8> = slice.iter()
                            .flat_map(|&s| {
                                let i = (s * 32_767.0).clamp(-32_768.0, 32_767.0) as i16;
                                i.to_le_bytes()
                            })
                            .collect();
                        if write.send(Message::Binary(bytes)).await.is_err() { return; }
                    }
                    batch.clear();
                }
            }
        }
        let _ = write.send(Message::Text(r#"{"type":"Terminate"}"#.into())).await;
        println!("\n[ws] Terminate sent");
    });

    // Receive + print all messages.
    println!("[ws] listening for transcripts...\n");
    while let Some(msg) = read.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                let val: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
                match val["type"].as_str() {
                    Some("Turn") => {
                        let transcript = val["transcript"].as_str().unwrap_or("");
                        let eot = val["end_of_turn"].as_bool().unwrap_or(false);
                        if eot {
                            // Final turn — print clearly
                            println!("\n[TRANSCRIPT] {transcript}");
                        } else if !transcript.is_empty() {
                            // Partial — print on same line
                            eprint!("\r[partial   ] {transcript:<80}");
                        }
                    }
                    Some("Termination") => {
                        println!("\n[ws] session terminated — audio: {}s",
                            val["audio_duration_seconds"].as_f64().unwrap_or(0.0));
                        break;
                    }
                    _ => {
                        // Print everything else raw so nothing is hidden.
                        println!("[ws ←] {text}");
                    }
                }
            }
            Ok(Message::Close(frame)) => {
                println!("\n[ws] server closed: {frame:?}");
                break;
            }
            Err(e) => {
                println!("\n[ws] error: {e}");
                break;
            }
            _ => {}
        }
    }

    stop.store(true, Ordering::Release);
    sender_handle.abort();
    drop(stream);
    println!("\ndone.");
}

// ── Audio helpers ─────────────────────────────────────────────────────────────

fn to_mono_f32(samples: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return samples.to_vec();
    }
    samples
        .chunks_exact(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect()
}

/// Box-filter downsampler: averages every `ratio` input samples.
/// For integer ratios (e.g. 48k→16k = 3) this is a proper anti-alias filter.
fn downsample_box(samples: &[f32], from_hz: u32, to_hz: u32) -> Vec<f32> {
    if from_hz == to_hz || samples.is_empty() {
        return samples.to_vec();
    }
    if from_hz % to_hz == 0 {
        let ratio = (from_hz / to_hz) as usize;
        return samples
            .chunks(ratio)
            .filter(|c| c.len() == ratio)
            .map(|c| c.iter().sum::<f32>() / ratio as f32)
            .collect();
    }
    // Non-integer ratio fallback.
    let ratio = from_hz as f64 / to_hz as f64;
    let out_len = ((samples.len() as f64) / ratio).ceil() as usize;
    let mut out = Vec::with_capacity(out_len);
    let mut pos = 0.0f64;
    while pos < samples.len() as f64 {
        let i = pos as usize;
        let frac = (pos - i as f64) as f32;
        let s0 = samples[i];
        let s1 = samples.get(i + 1).copied().unwrap_or(s0);
        out.push(s0 + (s1 - s0) * frac);
        pos += ratio;
    }
    out
}
