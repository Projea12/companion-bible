use std::path::PathBuf;

use sha1::{Digest, Sha1};

use crate::download::{download_if_needed, verify_sha1, DownloadConfig};
use crate::error::TranscriptionError;
use crate::model::{rss_mb, WhisperModel, GGML_MEDIUM_SHA1, MEMORY_BUDGET_MB};

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

    // Pre-populate the destination.
    let dest = dir.path().join("ggml-medium.bin");
    std::fs::write(&dest, data).unwrap();

    let cfg = DownloadConfig {
        url: "http://should-not-be-called".into(),
        sha1: expected,
        dest: dest.clone(),
    };

    let mut progress_values = Vec::new();
    download_if_needed(&cfg, |p| progress_values.push(p))
        .expect("should succeed when file is present with correct hash");

    // The function should not have downloaded anything; progress goes 0.5 → 1.0.
    assert!(
        progress_values.contains(&1.0_f32),
        "progress must reach 1.0"
    );
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

    let err = download_if_needed(&cfg, |_| {}).expect_err("bad hash must fail");
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

// ─── Full model tests (require model file — run manually) ─────────────────────

/// Load the model, run a health check, and report memory usage.
///
/// Run with:
/// ```sh
/// cargo test -p companion-transcription model_load -- --ignored --nocapture
/// ```
#[test]
#[ignore]
fn model_load_health_check_and_memory() {
    let model_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("models/whisper/ggml-medium.bin");

    assert!(
        model_path.exists(),
        "model not found at {model_path:?}\nRun: bash scripts/download_whisper.sh"
    );

    // Verify checksum before loading.
    println!("Verifying checksum...");
    verify_sha1(&model_path, GGML_MEDIUM_SHA1).expect("checksum must match");
    println!("✓ Checksum OK");

    // Load with a simple console progress indicator.
    println!("Loading model (this takes a few seconds)...");
    let model = WhisperModel::load(&model_path, |frac| {
        if frac == 0.0 {
            print!("  [loading]");
        } else if frac == 1.0 {
            println!(" done.");
        }
    })
    .expect("model must load without error");

    println!("  Load time   : {} ms", model.load_time_ms);
    println!("  Memory delta: {} MB", model.memory_delta_mb);

    // ── Memory budget ─────────────────────────────────────────────────────────
    model
        .assert_within_budget()
        .expect("model must fit within 4 GB memory budget");
    println!(
        "  Budget check: {} MB / {} MB — OK",
        model.memory_delta_mb, MEMORY_BUDGET_MB
    );

    // ── Health check ─────────────────────────────────────────────────────────
    println!("Running health check (0.1 s silence)...");
    let report = model.health_check().expect("health check must pass");
    println!("  Health OK   : {}", report.ok);
    println!("  Inference   : {} ms", report.inference_ms);
    println!("  Segments    : {}", report.n_segments);
    assert!(report.ok);
}
