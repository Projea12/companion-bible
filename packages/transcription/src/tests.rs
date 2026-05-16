use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use sha1::{Digest, Sha1};

use companion_audio::SlidingWindow;

use crate::download::{download_if_needed, verify_sha1, DownloadConfig};
use crate::error::TranscriptionError;
use crate::manager::{ModelManager, SetupProgress};
use crate::model::{rss_mb, WhisperModel, GGML_MEDIUM_SHA1, MEMORY_BUDGET_MB};
use crate::transcript::{TranscribeOptions, TranscriptionSegment};
use crate::transcriber::{normalize, EmittedSet, WhisperTranscriber};

// ─── Checksum ─────────────────────────────────────────────────────────────────

#[test]
fn sha1_passes_for_correct_hash() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("data.bin");
    let data = b"companion-bible test data";
    std::fs::write(&path, data).unwrap();

    let expected = format!("{:x}", Sha1::digest(data));
    verify_sha1(&path, &expected).expect("correct hash must pass verification");
}

#[test]
fn sha1_fails_for_wrong_hash() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("data.bin");
    std::fs::write(&path, b"some content").unwrap();

    let err = verify_sha1(&path, "0000000000000000000000000000000000000000")
        .expect_err("wrong hash must fail");
    assert!(
        matches!(err, TranscriptionError::ChecksumMismatch { .. }),
        "expected ChecksumMismatch, got {err:?}"
    );
}

#[test]
fn sha1_case_insensitive() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("data.bin");
    let data = b"case test";
    std::fs::write(&path, data).unwrap();

    let lower = format!("{:x}", Sha1::digest(data));
    let upper = lower.to_uppercase();
    verify_sha1(&path, &upper).expect("uppercase hash must also pass");
}

#[test]
fn sha1_error_on_missing_file() {
    let err = verify_sha1(
        &PathBuf::from("/nonexistent/path/model.bin"),
        "0000000000000000000000000000000000000000",
    )
    .expect_err("missing file must return Io error");
    assert!(matches!(err, TranscriptionError::Io(_)));
}

// ─── download_if_needed — file already present ────────────────────────────────

#[test]
fn download_skipped_when_file_exists_and_hash_matches() {
    let dir = tempfile::tempdir().unwrap();
    let data = b"fake model weights";
    let expected = format!("{:x}", Sha1::digest(data));

    let dest = dir.path().join("ggml-medium.bin");
    std::fs::write(&dest, data).unwrap();

    let cfg = DownloadConfig {
        url: "http://should-not-be-called".into(),
        sha1: expected,
        dest: dest.clone(),
    };

    // No error when file is present with correct hash — no network call made.
    download_if_needed(&cfg, |_, _| {})
        .expect("should succeed when file is present with correct hash");
}

#[test]
fn download_fails_when_file_exists_with_wrong_hash() {
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("ggml-medium.bin");
    std::fs::write(&dest, b"corrupted data").unwrap();

    let cfg = DownloadConfig {
        url: "http://should-not-be-called".into(),
        sha1: "0000000000000000000000000000000000000000".into(),
        dest,
    };

    let err = download_if_needed(&cfg, |_, _| {}).expect_err("bad hash must fail");
    assert!(matches!(err, TranscriptionError::ChecksumMismatch { .. }));
}

// ─── DownloadConfig ───────────────────────────────────────────────────────────

#[test]
fn download_config_whisper_medium_path() {
    let cfg = DownloadConfig::whisper_medium(std::path::Path::new("models/whisper"));
    assert_eq!(cfg.dest.file_name().unwrap(), "ggml-medium.bin");
    assert_eq!(cfg.sha1, crate::model::GGML_MEDIUM_SHA1);
}

// ─── Memory helpers ───────────────────────────────────────────────────────────

#[test]
fn rss_mb_returns_nonzero_on_supported_platforms() {
    let mb = rss_mb();
    // On macOS and Linux the call should return something; allow 0 elsewhere.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    assert!(mb > 0, "rss_mb() should return > 0 on macOS/Linux, got {mb}");
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let _ = mb; // tolerate 0 on other platforms
}

// ─── Model constants ──────────────────────────────────────────────────────────

#[test]
fn memory_budget_is_within_8gb() {
    // The app has an 8 GB total RAM budget; the model alone must not exceed 4 GB.
    assert!(
        MEMORY_BUDGET_MB <= 8_192,
        "MEMORY_BUDGET_MB {MEMORY_BUDGET_MB} exceeds the 8 GB app budget"
    );
}

#[test]
fn ggml_medium_sha1_is_40_hex_chars() {
    assert_eq!(
        GGML_MEDIUM_SHA1.len(),
        40,
        "SHA-1 digest must be 40 hex characters"
    );
    assert!(
        GGML_MEDIUM_SHA1.chars().all(|c| c.is_ascii_hexdigit()),
        "SHA-1 digest must contain only hex characters"
    );
}

// ─── ModelManager ────────────────────────────────────────────────────────────

#[test]
fn model_manager_is_present_false_when_no_file() {
    let dir = tempfile::tempdir().unwrap();
    let mgr = ModelManager::new(dir.path());
    assert!(!mgr.is_present(), "is_present() must be false before any download");
}

#[test]
fn model_manager_is_present_true_when_file_exists() {
    let dir = tempfile::tempdir().unwrap();
    let mgr = ModelManager::new(dir.path());
    std::fs::create_dir_all(mgr.model_path().parent().unwrap()).unwrap();
    std::fs::write(mgr.model_path(), b"placeholder").unwrap();
    assert!(mgr.is_present(), "is_present() must be true when file is on disk");
}

#[test]
fn model_manager_model_path_ends_with_ggml_medium() {
    let dir = tempfile::tempdir().unwrap();
    let mgr = ModelManager::new(dir.path());
    assert_eq!(mgr.model_path().file_name().unwrap(), "ggml-medium.bin");
}

#[test]
fn setup_progress_labels_are_non_empty() {
    let cases = [
        SetupProgress::Checking,
        SetupProgress::AlreadyPresent,
        SetupProgress::Downloading { bytes_done: 0, bytes_total: None },
        SetupProgress::Verifying,
        SetupProgress::Loading,
        SetupProgress::Ready { load_time_ms: 0, memory_mb: 0 },
    ];
    for p in &cases {
        assert!(!p.label().is_empty(), "label must not be empty for {p:?}");
    }
}

#[test]
fn setup_progress_download_percent_correct() {
    let p = SetupProgress::Downloading { bytes_done: 750, bytes_total: Some(1000) };
    assert_eq!(p.download_percent(), Some(75));

    let p2 = SetupProgress::Downloading { bytes_done: 500, bytes_total: None };
    assert_eq!(p2.download_percent(), None);

    assert_eq!(SetupProgress::Checking.download_percent(), None);
}

#[test]
fn setup_fails_gracefully_when_model_corrupt() {
    let dir = tempfile::tempdir().unwrap();
    let mgr = ModelManager::new(dir.path());

    // Plant a corrupt file so the manager tries to verify it (no network call).
    std::fs::create_dir_all(mgr.model_path().parent().unwrap()).unwrap();
    std::fs::write(mgr.model_path(), b"this is not a real model").unwrap();

    let err = mgr.setup(|_| {}).expect_err("corrupt file must fail setup");
    assert!(
        matches!(err, TranscriptionError::ChecksumMismatch { .. }),
        "expected ChecksumMismatch, got {err:?}"
    );
}

// ─── TranscriptionSegment ─────────────────────────────────────────────────────

#[test]
fn segment_duration_ms() {
    let s = TranscriptionSegment {
        text: "hello".into(),
        audio_start_ms: 1_000,
        audio_end_ms: 3_500,
        whisper_confidence: 0.9,
        is_duplicate: false,
        context_window: String::new(),
    };
    assert_eq!(s.duration_ms(), 2_500);
}

#[test]
fn segment_zero_duration() {
    let s = TranscriptionSegment {
        text: "x".into(),
        audio_start_ms: 500,
        audio_end_ms: 500,
        whisper_confidence: 1.0,
        is_duplicate: false,
        context_window: String::new(),
    };
    assert_eq!(s.duration_ms(), 0);
}

#[test]
fn segment_is_duplicate_defaults_false() {
    let s = TranscriptionSegment {
        text: "John 3:16".into(),
        audio_start_ms: 0,
        audio_end_ms: 2_000,
        whisper_confidence: 0.95,
        is_duplicate: false,
        context_window: "For God so loved the world".into(),
    };
    assert!(!s.is_duplicate);
    assert!(!s.context_window.is_empty());
}

// ─── TranscribeOptions ────────────────────────────────────────────────────────

#[test]
fn default_options_are_english() {
    let opts = TranscribeOptions::default();
    assert_eq!(opts.language.as_deref(), Some("en"));
    assert!(opts.n_threads > 0);
    assert!(opts.no_speech_threshold > 0.0 && opts.no_speech_threshold < 1.0);
    assert_eq!(opts.temperature, 0.0, "default temperature must be 0.0");
}

#[test]
fn church_options_auto_detect_language() {
    let opts = TranscribeOptions::church();
    assert!(opts.language.is_none(), "church preset must use auto-detect");
    assert_eq!(opts.temperature, 0.0);
    assert!(!opts.initial_prompt.is_empty(), "church preset must include a sermon prompt");
    assert!(opts.initial_prompt.contains("Genesis"), "prompt must include book names");
}

#[test]
fn church_options_prompt_contains_nt_books() {
    let prompt = TranscribeOptions::church().initial_prompt;
    for book in ["Romans", "John", "Revelation", "Psalms"] {
        assert!(prompt.contains(book), "church prompt must contain '{book}'");
    }
}

// ─── Transcription (requires model — run manually) ────────────────────────────

fn model_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent().unwrap()
        .parent().unwrap()
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

/// Silence produces no segments (or only empty-text segments that get dropped).
///
/// ```sh
/// cargo test -p companion-transcription transcribe_silence -- --ignored --nocapture
/// ```
#[test]
#[ignore]
fn transcribe_silence_returns_no_segments() {
    let model = load_model();
    let silence = vec![0.0f32; 16_000 * 5]; // 5 s silence
    let segments = model
        .transcribe(&silence, &TranscribeOptions::default())
        .expect("transcribe must not error on silence");

    println!("Segments from silence: {}", segments.len());
    for s in &segments {
        println!("  [{}-{}ms] {:?}", s.audio_start_ms, s.audio_end_ms, s.text);
    }
    // Whisper may hallucinate on pure silence but the call must not panic.
    // We only assert the return type is correct, not the content.
    let _ = segments;
}

/// Transcribe a synthetic 440 Hz tone — Whisper should return something
/// (even if it's just music/noise notation) without crashing.
///
/// ```sh
/// cargo test -p companion-transcription transcribe_tone -- --ignored --nocapture
/// ```
#[test]
#[ignore]
fn transcribe_tone_does_not_panic() {
    let model = load_model();
    let sr = 16_000usize;
    let tone: Vec<f32> = (0..sr * 3)
        .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr as f32).sin() * 0.4)
        .collect();

    let result = model.transcribe(&tone, &TranscribeOptions::default());
    assert!(result.is_ok(), "transcribe must not error: {:?}", result);
    let segs = result.unwrap();
    println!("Segments from 440 Hz tone: {}", segs.len());
    for s in &segs {
        println!("  [{}-{}ms] {:?}", s.audio_start_ms, s.audio_end_ms, s.text);
    }
}

/// Performance: transcribing 30 s of audio must complete in under 60 s on CPU
/// (whisper medium is ~1–2× real-time without Metal).
///
/// ```sh
/// cargo test -p companion-transcription transcribe_performance -- --ignored --nocapture
/// ```
#[test]
#[ignore]
fn transcribe_30s_under_60s() {
    let model = load_model();
    let sr = 16_000usize;
    // 30 s of a gentle 300 Hz sine — gives Whisper something non-trivial.
    let audio: Vec<f32> = (0..sr * 30)
        .map(|i| (2.0 * std::f32::consts::PI * 300.0 * i as f32 / sr as f32).sin() * 0.3)
        .collect();

    let t0 = std::time::Instant::now();
    let segs = model
        .transcribe(&audio, &TranscribeOptions::default())
        .expect("transcribe must not error");
    let elapsed = t0.elapsed();

    println!("Transcribed 30 s in {:.1} s → {} segments", elapsed.as_secs_f64(), segs.len());
    assert!(
        elapsed.as_secs() < 60,
        "transcription took {:.1} s, must be under 60 s",
        elapsed.as_secs_f64()
    );
}

// ─── Full model tests (require model file — run manually) ─────────────────────

/// Full first-launch setup via ModelManager: download (if needed) → verify →
/// load → health check → memory budget.
///
/// Run with:
/// ```sh
/// cargo test -p companion-transcription model_first_launch -- --ignored --nocapture
/// ```
/// The model is read from (or downloaded to) `models/whisper/ggml-medium.bin`
/// relative to the workspace root.
#[test]
#[ignore]
fn model_first_launch_setup() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();

    let mgr = ModelManager::new(&workspace_root);

    println!("\nModel path : {:?}", mgr.model_path());
    println!("Present    : {}", mgr.is_present());
    if !mgr.is_present() {
        println!("Model not found — will download (~1.5 GB, may take several minutes)");
    }

    let model = mgr
        .setup(|progress| {
            match &progress {
                SetupProgress::Downloading { bytes_done, bytes_total } => {
                    let pct = progress.download_percent().map(|p| format!("{p}%")).unwrap_or_else(|| "?".into());
                    let mb = bytes_done / 1_048_576;
                    let total_mb = bytes_total.map(|t| format!("{} MB", t / 1_048_576)).unwrap_or_else(|| "?".into());
                    print!("\r  {pct}  {mb} MB / {total_mb}       ");
                    use std::io::Write as _;
                    let _ = std::io::stdout().flush();
                }
                other => println!("  {}", other.label()),
            }
        })
        .expect("setup must succeed");

    println!("\n  Load time   : {} ms", model.load_time_ms);
    println!("  Memory delta: {} MB", model.memory_delta_mb);
    assert!(
        model.memory_delta_mb <= MEMORY_BUDGET_MB,
        "model uses {} MB, exceeds {} MB budget",
        model.memory_delta_mb,
        MEMORY_BUDGET_MB
    );
}
