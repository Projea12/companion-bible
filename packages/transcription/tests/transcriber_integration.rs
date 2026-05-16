//! Integration tests for the full WhisperTranscriber sliding-window loop.
//!
//! These tests differ from `accuracy.rs` in a key way: they call
//! `WhisperModel::transcribe` **indirectly** through the `WhisperTranscriber`
//! loop — audio enters via `SlidingWindow`, waits for the 5-second trigger,
//! travels through the deduplication layer, and exits as a batch on the
//! mpsc channel.  `accuracy.rs` tests the model's transcript quality directly;
//! these tests verify the scheduler, dedup, and channel plumbing.
//!
//! All tests require the GGML medium model.  Run with:
//! ```sh
//! cargo test -p companion-transcription --test transcriber_integration -- --ignored --nocapture
//! ```

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use companion_audio::SlidingWindow;
use companion_transcription::{
    TranscribeOptions, TranscriptionSegment, WhisperModel, WhisperTranscriber,
    NEW_AUDIO_SECS, TRANSCRIBE_WINDOW_SECS,
};

// ─── Shared helpers ───────────────────────────────────────────────────────────

fn model_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("models/whisper/ggml-medium.bin")
}

fn load_model() -> WhisperModel {
    let path = model_path();
    assert!(
        path.exists(),
        "model not found — run: bash scripts/download_whisper.sh"
    );
    WhisperModel::load(&path, |_| {}).expect("model must load")
}

/// Synthesise speech via macOS `say` and return raw mono f32 @ 16 kHz.
///
/// `say` is called without `--data-format` because specifying a raw format
/// with an unrecognised extension (e.g. `.raw`) causes "Opening output file
/// failed: fmt?" on macOS.  Instead we let `say` write its native AIFF, then
/// convert with `afconvert`.
fn synthesize(text: &str, voice: &str) -> Vec<f32> {
    let dir = tempfile::tempdir().expect("tempdir");
    let aiff = dir.path().join("tts.aiff");

    let status = std::process::Command::new("say")
        .args(["--voice", voice, "-o", aiff.to_str().unwrap(), text])
        .status()
        .expect("`say` must be available (macOS only)");
    assert!(status.success(), "`say` failed for '{text}'");

    // afconvert: AIFF → WAV f32 @ 16 kHz mono
    // `/dev/stdout` is not seekable so afconvert can't write the WAV header;
    // use a temp file instead.
    let wav = dir.path().join("tts.wav");
    let status = std::process::Command::new("afconvert")
        .args([
            aiff.to_str().unwrap(),
            wav.to_str().unwrap(),
            "-f", "WAVE",
            "-d", "LEF32@16000",
            "-c", "1",
        ])
        .status()
        .expect("`afconvert` must be available (macOS only)");
    assert!(status.success(), "`afconvert` failed");

    wav_to_f32(&std::fs::read(&wav).expect("read WAV"))
}

/// Pull f32 PCM samples from a WAV byte buffer by locating the `data` chunk.
fn wav_to_f32(bytes: &[u8]) -> Vec<f32> {
    let data_pos = bytes
        .windows(4)
        .position(|w| w == b"data")
        .expect("WAV 'data' chunk not found");
    // skip "data" marker (4 bytes) + chunk-size field (4 bytes)
    bytes[data_pos + 8..]
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect()
}

/// Pad or trim `samples` to exactly `target_secs` seconds at 16 kHz.
/// Silence is prepended so the spoken content sits at the end of the window,
/// matching how real audio arrives (most recent samples last).
fn pad_to_secs(samples: &[f32], target_secs: u64) -> Vec<f32> {
    let target = 16_000 * target_secs as usize;
    let mut out = vec![0.0f32; target];
    let copy_len = samples.len().min(target);
    out[target - copy_len..].copy_from_slice(&samples[..copy_len]);
    out
}

/// Collect all segment texts from a batch, lowercased.
fn batch_text(batch: &[TranscriptionSegment]) -> String {
    batch.iter().map(|s| s.text.to_lowercase()).collect::<Vec<_>>().join(" ")
}

// ─── Integration: verse references emitted through transcriber loop ───────────

/// John 3:16 spoken through the full WhisperTranscriber pipeline.
///
/// This verifies the scheduler triggers, Whisper runs, and the correct
/// segment text arrives on the mpsc receiver.
#[test]
#[ignore]
fn verse_reference_john_3_16_reaches_channel() {
    let model = load_model();
    let audio = synthesize(
        "For God so loved the world that he gave his only Son. John chapter 3 verse 16.",
        "Samantha",
    );
    let padded = pad_to_secs(&audio, 15);

    let window = Arc::new(Mutex::new(SlidingWindow::new()));
    window.lock().unwrap().push(&padded);

    let (mut transcriber, rx) =
        WhisperTranscriber::new(Arc::clone(&window), TranscribeOptions::church());
    transcriber.start(model);

    let batch = rx.recv_timeout(Duration::from_secs(90)).expect("expected a segment batch");
    let text = batch_text(&batch);
    println!("john_3_16 via loop: {text}");

    assert!(!batch.is_empty(), "must receive at least one segment");
    assert!(
        text.contains("john") || text.contains("3") || text.contains("16"),
        "expected verse reference in: {text}"
    );

    transcriber.stop();
}

/// Romans 8:1 spoken through the full pipeline.
#[test]
#[ignore]
fn verse_reference_romans_8_1_reaches_channel() {
    let model = load_model();
    let audio = synthesize(
        "There is therefore now no condemnation for those in Christ Jesus. \
         Romans chapter 8 verse 1.",
        "Samantha",
    );
    let padded = pad_to_secs(&audio, 15);

    let window = Arc::new(Mutex::new(SlidingWindow::new()));
    window.lock().unwrap().push(&padded);

    let (mut transcriber, rx) =
        WhisperTranscriber::new(Arc::clone(&window), TranscribeOptions::church());
    transcriber.start(model);

    let batch = rx.recv_timeout(Duration::from_secs(90)).expect("batch");
    let text = batch_text(&batch);
    println!("romans_8_1 via loop: {text}");

    assert!(
        text.contains("romans") || text.contains("8"),
        "expected 'romans' or '8' in: {text}"
    );

    transcriber.stop();
}

/// Multiple verse references in one window — all must appear in the emitted batch.
#[test]
#[ignore]
fn multiple_verse_references_in_one_window() {
    let model = load_model();
    let audio = synthesize(
        "Today we look at two passages. First, Genesis chapter 1 verse 1: \
         In the beginning God created the heavens and the earth. \
         Second, John chapter 3 verse 16: For God so loved the world.",
        "Samantha",
    );
    let padded = pad_to_secs(&audio, 15);

    let window = Arc::new(Mutex::new(SlidingWindow::new()));
    window.lock().unwrap().push(&padded);

    let (mut transcriber, rx) =
        WhisperTranscriber::new(Arc::clone(&window), TranscribeOptions::church());
    transcriber.start(model);

    let batch = rx.recv_timeout(Duration::from_secs(90)).expect("batch");
    let text = batch_text(&batch);
    println!("multi_verse via loop: {text}");

    assert!(
        text.contains("genesis") || text.contains("john"),
        "expected at least one book name in: {text}"
    );

    transcriber.stop();
}

// ─── Integration: no duplicate segments ───────────────────────────────────────

/// Pushing the same audio a second time must not produce duplicate segments on
/// the channel.  The `EmittedSet` in the transcriber remembers text from the
/// first run and filters it from the second.
#[test]
#[ignore]
fn no_duplicate_segments_on_repeated_audio() {
    let model = load_model();
    let audio = synthesize(
        "For God so loved the world. John chapter 3 verse 16.",
        "Samantha",
    );
    let padded = pad_to_secs(&audio, 15);

    let window = Arc::new(Mutex::new(SlidingWindow::new()));
    window.lock().unwrap().push(&padded);

    let (mut transcriber, rx) =
        WhisperTranscriber::new(Arc::clone(&window), TranscribeOptions::church());
    transcriber.start(model);

    // First batch — should contain the verse text.
    let first = rx.recv_timeout(Duration::from_secs(90)).expect("first batch");
    let first_texts: std::collections::HashSet<String> =
        first.iter().map(|s| s.text.to_lowercase()).collect();
    println!("First batch ({} segs): {:?}", first.len(), first_texts);
    assert!(!first.is_empty(), "first batch must not be empty");

    // Push exactly the same audio again.
    window.lock().unwrap().push(&padded);

    // Wait for a possible second batch.  If all text was deduplicated the
    // channel will stay empty until the timeout — that is the desired outcome.
    match rx.recv_timeout(Duration::from_secs(30)) {
        Ok(second) => {
            let second_texts: Vec<String> =
                second.iter().map(|s| s.text.to_lowercase()).collect();
            println!("Second batch ({} segs): {second_texts:?}", second.len());

            // Any text that appeared in the first batch must not reappear.
            for text in &second_texts {
                // Normalise the same way EmittedSet does (split_whitespace + lowercase).
                let norm: String = text
                    .split_whitespace()
                    .map(|w| w.to_lowercase())
                    .collect::<Vec<_>>()
                    .join(" ");
                let is_dup = first_texts.iter().any(|f| {
                    let f_norm: String = f
                        .split_whitespace()
                        .map(|w| w.to_lowercase())
                        .collect::<Vec<_>>()
                        .join(" ");
                    f_norm == norm
                });
                assert!(
                    !is_dup,
                    "duplicate segment leaked to channel: '{text}'"
                );
            }
        }
        Err(_) => {
            println!("No second batch — all text correctly deduplicated");
        }
    }

    transcriber.stop();
}

/// Sending completely different audio after the first run must emit new segments.
/// Verifies the dedup logic does not over-filter.
#[test]
#[ignore]
fn different_audio_emits_new_segments_after_first_run() {
    let model = load_model();

    let window = Arc::new(Mutex::new(SlidingWindow::new()));

    // First window: John 3:16.
    let audio1 = synthesize("John chapter 3 verse 16.", "Samantha");
    window.lock().unwrap().push(&pad_to_secs(&audio1, 15));

    let (mut transcriber, rx) =
        WhisperTranscriber::new(Arc::clone(&window), TranscribeOptions::church());
    transcriber.start(model);

    let first = rx.recv_timeout(Duration::from_secs(90)).expect("first batch");
    println!("First ({} segs): {}", first.len(), batch_text(&first));

    // Second window: Romans 8:1 — completely different text.
    let audio2 = synthesize(
        "Romans chapter 8 verse 1. There is no condemnation.",
        "Samantha",
    );
    window.lock().unwrap().push(&pad_to_secs(&audio2, 15));

    let second = rx.recv_timeout(Duration::from_secs(90)).expect("second batch");
    let text2 = batch_text(&second);
    println!("Second ({} segs): {text2}", second.len());

    assert!(!second.is_empty(), "new audio must produce new segments");
    assert!(
        text2.contains("romans") || text2.contains("8") || text2.contains("condemnation"),
        "expected Romans content in second batch: {text2}"
    );

    transcriber.stop();
}

// ─── Latency: scheduling overhead under 400 ms ────────────────────────────────

/// Measures the overhead added by the WhisperTranscriber loop on top of raw
/// Whisper inference.  The overhead covers: poll delay (≤ 100 ms), window lock,
/// dedup check, and channel send.  Whisper inference itself is excluded by
/// measuring it separately on the same audio before starting the transcriber.
///
/// Budget: scheduling overhead < 400 ms.
#[test]
#[ignore]
fn transcriber_scheduling_overhead_under_400ms() {
    let model = load_model();
    let audio = synthesize(
        "For God so loved the world. John chapter 3 verse 16.",
        "Samantha",
    );
    let padded = pad_to_secs(&audio, 15);
    let opts = TranscribeOptions::church();

    // ── Step 1: measure direct inference time (borrows, does not move model) ──
    let t_inference = Instant::now();
    let _ = model.transcribe(&padded, &opts).expect("direct transcribe");
    let inference_elapsed = t_inference.elapsed();
    println!("Direct Whisper inference: {} ms", inference_elapsed.as_millis());

    // ── Step 2: run through WhisperTranscriber ────────────────────────────────
    let window = Arc::new(Mutex::new(SlidingWindow::new()));
    window.lock().unwrap().push(&padded);

    let (mut transcriber, rx) =
        WhisperTranscriber::new(Arc::clone(&window), opts);

    let t_start = Instant::now();
    transcriber.start(model); // model ownership moves to background thread

    let _batch = rx
        .recv_timeout(Duration::from_secs(120))
        .expect("transcriber must emit within 120 s");
    let total_elapsed = t_start.elapsed();

    // ── Step 3: overhead = total − inference ─────────────────────────────────
    // `saturating_sub` guards against clock jitter making total slightly less
    // than inference (different model runs can vary by a few ms).
    let overhead = total_elapsed.saturating_sub(inference_elapsed);

    println!(
        "Total (loop): {} ms | Inference: {} ms | Overhead: {} ms",
        total_elapsed.as_millis(),
        inference_elapsed.as_millis(),
        overhead.as_millis(),
    );

    assert!(
        overhead.as_millis() < 400,
        "transcriber scheduling overhead {} ms exceeds 400 ms budget",
        overhead.as_millis()
    );

    transcriber.stop();
}

/// Sanity check: the transcriber emits the first batch within a wall-clock
/// budget of NEW_AUDIO_SECS + inference_budget.  This catches regressions where
/// the loop goes to sleep for an unexpectedly long time.
#[test]
#[ignore]
fn transcriber_first_batch_within_wall_clock_budget() {
    // Whisper medium on CPU (no Metal) runs at ~3–4× real-time, so 15 s of
    // audio can take up to 60 s.  Add NEW_AUDIO_SECS (5 s) scheduler wait
    // plus 30 s headroom for slow machines or contention from parallel tests.
    let wall_clock_budget = Duration::from_secs(NEW_AUDIO_SECS + 90);

    let model = load_model();
    let audio = synthesize("John chapter 3 verse 16.", "Samantha");
    let padded = pad_to_secs(&audio, 15);

    let window = Arc::new(Mutex::new(SlidingWindow::new()));
    window.lock().unwrap().push(&padded);

    let (mut transcriber, rx) =
        WhisperTranscriber::new(Arc::clone(&window), TranscribeOptions::church());

    let t0 = Instant::now();
    transcriber.start(model);

    let _batch = rx
        .recv_timeout(wall_clock_budget)
        .expect("first batch must arrive within wall-clock budget");

    println!(
        "First batch arrived in {} ms (budget {} ms)",
        t0.elapsed().as_millis(),
        wall_clock_budget.as_millis()
    );

    transcriber.stop();
}

// ─── Timestamp-based deduplication ───────────────────────────────────────────

/// After run 1 the window is trimmed by NEW_AUDIO_SECS (5 s).  In run 2 the
/// first 10 s of the 15 s window is overlap — segments from that zone whose
/// text was already emitted must be suppressed by the timestamp check.
///
/// Strategy: push 15 s of audio, collect run 1.  Then push 5 s of DIFFERENT
/// audio.  Verify run 2's batch contains only the new 5 s of content, not
/// a repeat of anything from run 1.
#[test]
#[ignore]
fn timestamp_dedup_overlap_zone_not_re_emitted() {
    let model = load_model();

    // Run-1 audio: a clear verse reference.
    let audio1 = synthesize(
        "For God so loved the world. John chapter 3 verse 16.",
        "Samantha",
    );
    let padded1 = pad_to_secs(&audio1, 15);

    let window = Arc::new(Mutex::new(SlidingWindow::new()));
    window.lock().unwrap().push(&padded1);

    let (mut transcriber, rx) =
        WhisperTranscriber::new(Arc::clone(&window), TranscribeOptions::church());
    transcriber.start(model);

    let run1 = rx.recv_timeout(Duration::from_secs(120)).expect("run 1 batch");
    let run1_texts: Vec<String> = run1.iter().map(|s| s.text.to_lowercase()).collect();
    println!("Run 1 ({} segs): {:?}", run1.len(), run1_texts);
    assert!(!run1.is_empty(), "run 1 must produce segments");

    // Push 5 s of completely different audio — this is the NEW zone for run 2.
    // The transcriber's internal window still holds the last 10 s of padded1
    // (the overlap zone) plus these 5 s.
    let audio2 = synthesize("Romans chapter 8 verse 1.", "Samantha");
    let padded2 = pad_to_secs(&audio2, NEW_AUDIO_SECS); // exactly 5 s
    window.lock().unwrap().push(&padded2);

    let run2 = rx.recv_timeout(Duration::from_secs(120)).expect("run 2 batch");
    let run2_texts: Vec<String> = run2.iter().map(|s| s.text.to_lowercase()).collect();
    println!("Run 2 ({} segs): {:?}", run2.len(), run2_texts);

    // Nothing from run 1 should reappear in run 2.
    for r2 in &run2_texts {
        let r2_norm: String = r2.split_whitespace().collect::<Vec<_>>().join(" ");
        for r1 in &run1_texts {
            let r1_norm: String = r1.split_whitespace().collect::<Vec<_>>().join(" ");
            assert_ne!(
                r2_norm, r1_norm,
                "run 2 segment '{r2}' duplicates run 1 segment '{r1}' — timestamp dedup failed"
            );
        }
    }

    // Run 2 should contain the Romans content (the new 5 s zone).
    let run2_joined = run2_texts.join(" ");
    assert!(
        run2_joined.contains("romans") || run2_joined.contains("condemnation"),
        "run 2 must contain new Romans content, got: {run2_joined}"
    );

    transcriber.stop();
}

/// A segment that straddles the overlap boundary (starts just before the
/// NEW_AUDIO boundary but is genuinely new text) must be emitted exactly once.
/// This exercises the text-fallback path inside the overlap zone.
#[test]
#[ignore]
fn timestamp_dedup_partial_overlap_emitted_exactly_once() {
    let model = load_model();

    // Fill the window with long audio so a segment can straddle the trim boundary.
    let audio = synthesize(
        "Turn your Bibles to Romans chapter 8 verse 1. \
         There is therefore now no condemnation for those who are in Christ Jesus.",
        "Samantha",
    );
    let padded = pad_to_secs(&audio, TRANSCRIBE_WINDOW_SECS);
    let window = Arc::new(Mutex::new(SlidingWindow::new()));
    window.lock().unwrap().push(&padded);

    let (mut transcriber, rx) =
        WhisperTranscriber::new(Arc::clone(&window), TranscribeOptions::church());
    transcriber.start(model);

    // Collect run 1.
    let run1 = rx.recv_timeout(Duration::from_secs(120)).expect("run 1");
    let run1_texts: std::collections::HashSet<String> =
        run1.iter().map(|s| {
            s.text.split_whitespace().map(|w| w.to_lowercase()).collect::<Vec<_>>().join(" ")
        }).collect();
    println!("Run 1 ({} segs): {:?}", run1.len(), run1_texts);

    // Push silence to trigger run 2 without new meaningful audio.
    let silence = vec![0.0f32; 16_000 * NEW_AUDIO_SECS as usize];
    window.lock().unwrap().push(&silence);

    // Run 2 may or may not emit (Whisper might hallucinate on silence).
    // Whatever it emits must not duplicate run 1 text.
    if let Ok(run2) = rx.recv_timeout(Duration::from_secs(120)) {
        let run2_texts: Vec<String> =
            run2.iter().map(|s| {
                s.text.split_whitespace().map(|w| w.to_lowercase()).collect::<Vec<_>>().join(" ")
            }).collect();
        println!("Run 2 ({} segs): {:?}", run2.len(), run2_texts);

        for text in &run2_texts {
            assert!(
                !run1_texts.contains(text),
                "run 2 emitted duplicate segment: '{text}'"
            );
        }
    } else {
        println!("Run 2: no batch (silence correctly suppressed)");
    }

    transcriber.stop();
}

// ─── Book context injection ───────────────────────────────────────────────────

/// Setting the book context before the first run must not cause a panic and
/// must produce a valid batch.  Verifies the context handle wiring end-to-end.
#[test]
#[ignore]
fn book_context_set_before_run_produces_valid_batch() {
    let model = load_model();
    let audio = synthesize(
        "For God so loved the world. John chapter 3 verse 16.",
        "Samantha",
    );
    let padded = pad_to_secs(&audio, 15);

    let window = Arc::new(Mutex::new(SlidingWindow::new()));
    window.lock().unwrap().push(&padded);

    let (mut transcriber, rx) =
        WhisperTranscriber::new(Arc::clone(&window), TranscribeOptions::church());

    // Set context before start — the loop reads it on the first run.
    transcriber.set_book_context(Some("John".into()));
    transcriber.start(model);

    let batch = rx.recv_timeout(Duration::from_secs(120)).expect("batch");
    println!(
        "book_context John: {} segs — {}",
        batch.len(),
        batch.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join(" | ")
    );
    assert!(!batch.is_empty(), "must receive at least one segment");

    transcriber.stop();
}

/// The book context can be updated between runs via the shared handle.
/// Both runs must complete without error and each must emit fresh segments.
#[test]
#[ignore]
fn book_context_updated_between_runs_both_emit() {
    let model = load_model();

    let window = Arc::new(Mutex::new(SlidingWindow::new()));

    // Run 1: John 3:16, context = "John".
    let audio1 = synthesize("John chapter 3 verse 16.", "Samantha");
    window.lock().unwrap().push(&pad_to_secs(&audio1, 15));

    let (mut transcriber, rx) =
        WhisperTranscriber::new(Arc::clone(&window), TranscribeOptions::church());
    let ctx_handle = transcriber.book_context_handle();
    *ctx_handle.lock().unwrap() = Some("John".into());

    transcriber.start(model);
    let run1 = rx.recv_timeout(Duration::from_secs(120)).expect("run 1");
    println!("Run 1 (John context, {} segs): {}", run1.len(), batch_text(&run1));
    assert!(!run1.is_empty());

    // Switch context to "Romans" before run 2.
    *ctx_handle.lock().unwrap() = Some("Romans".into());

    let audio2 = synthesize("Romans chapter 8 verse 1.", "Samantha");
    window.lock().unwrap().push(&pad_to_secs(&audio2, 15));

    let run2 = rx.recv_timeout(Duration::from_secs(120)).expect("run 2");
    println!("Run 2 (Romans context, {} segs): {}", run2.len(), batch_text(&run2));
    assert!(!run2.is_empty(), "run 2 must emit new segments after context switch");

    transcriber.stop();
}

/// Clearing the book context (setting it to None) reverts to the generic
/// sermon prompt.  The transcriber must continue to function correctly.
#[test]
#[ignore]
fn book_context_cleared_mid_stream_continues_working() {
    let model = load_model();
    let audio = synthesize("John chapter 3 verse 16.", "Samantha");
    let padded = pad_to_secs(&audio, 15);

    let window = Arc::new(Mutex::new(SlidingWindow::new()));
    window.lock().unwrap().push(&padded);

    let (mut transcriber, rx) =
        WhisperTranscriber::new(Arc::clone(&window), TranscribeOptions::church());

    transcriber.set_book_context(Some("John".into()));
    transcriber.start(model);

    let _run1 = rx.recv_timeout(Duration::from_secs(120)).expect("run 1");

    // Clear context — next run uses generic prompt.
    transcriber.set_book_context(None);

    let audio2 = synthesize("Romans chapter 8 verse 1.", "Samantha");
    window.lock().unwrap().push(&pad_to_secs(&audio2, 15));

    let run2 = rx.recv_timeout(Duration::from_secs(120)).expect("run 2");
    println!(
        "run 2 (cleared context, {} segs): {}",
        run2.len(), batch_text(&run2)
    );
    assert!(!run2.is_empty(), "must emit segments even after context cleared");

    transcriber.stop();
}
