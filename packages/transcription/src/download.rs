use std::io::Read as _;
use std::path::{Path, PathBuf};

use sha1::{Digest, Sha1};

use crate::error::TranscriptionError;
use crate::model::{GGML_MEDIUM_SHA1, GGML_MEDIUM_URL};

// ─── DownloadConfig ───────────────────────────────────────────────────────────

/// Where to get a model and how to verify it.
pub struct DownloadConfig {
    /// HTTP(S) source URL.
    pub url: String,
    /// Expected SHA-1 hex digest (lowercase, 40 characters).
    pub sha1: String,
    /// Destination path on disk.
    pub dest: PathBuf,
}

impl DownloadConfig {
    /// Config for `ggml-medium.bin` placed in `models_dir`.
    pub fn whisper_medium(models_dir: &Path) -> Self {
        Self {
            url: GGML_MEDIUM_URL.to_string(),
            sha1: GGML_MEDIUM_SHA1.to_string(),
            dest: models_dir.join("ggml-medium.bin"),
        }
    }
}

// ─── download_if_needed ───────────────────────────────────────────────────────

/// Download the model if the destination file does not exist, then verify the
/// SHA-1 checksum.
///
/// `on_progress` receives a fraction in [0, 1]:
/// - `0.0` — starting
/// - `0.5` — file exists; verifying checksum
/// - `0.9` — download complete; verifying
/// - `1.0` — done
///
/// The download is performed via `curl` (must be on `PATH`).  A `.tmp`
/// extension is used during transfer; the file is renamed to `dest` only after
/// the checksum passes.
pub fn download_if_needed<F>(
    cfg: &DownloadConfig,
    on_progress: F,
) -> Result<(), TranscriptionError>
where
    F: Fn(f32),
{
    if cfg.dest.exists() {
        on_progress(0.5);
        verify_sha1(&cfg.dest, &cfg.sha1)?;
        on_progress(1.0);
        return Ok(());
    }

    if let Some(parent) = cfg.dest.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let tmp = cfg.dest.with_extension("tmp");
    on_progress(0.0);
    download_via_curl(&cfg.url, &tmp)?;
    on_progress(0.9);

    verify_sha1(&tmp, &cfg.sha1).map_err(|e| {
        let _ = std::fs::remove_file(&tmp); // clean up corrupt download
        e
    })?;

    std::fs::rename(&tmp, &cfg.dest)?;
    on_progress(1.0);
    Ok(())
}

// ─── verify_sha1 ─────────────────────────────────────────────────────────────

/// Stream `path` through SHA-1 and verify it matches `expected` (hex, lowercase).
pub fn verify_sha1(path: &Path, expected: &str) -> Result<(), TranscriptionError> {
    let file = std::fs::File::open(path)?;
    let mut reader = std::io::BufReader::new(file);
    let mut hasher = Sha1::new();
    let mut buf = [0u8; 65_536];

    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }

    let actual = format!("{:x}", hasher.finalize());
    if actual != expected.to_ascii_lowercase() {
        return Err(TranscriptionError::ChecksumMismatch {
            expected: expected.to_string(),
            actual,
        });
    }
    Ok(())
}

// ─── Internal ─────────────────────────────────────────────────────────────────

fn download_via_curl(url: &str, dest: &Path) -> Result<(), TranscriptionError> {
    let dest_str = dest
        .to_str()
        .ok_or_else(|| TranscriptionError::Download("non-UTF-8 destination path".into()))?;

    let status = std::process::Command::new("curl")
        .args([
            "--location",    // follow redirects (HuggingFace uses them)
            "--fail",        // non-zero exit on HTTP errors
            "--progress-bar", // visible progress in the terminal
            "--output",
            dest_str,
            url,
        ])
        .status()
        .map_err(|e| TranscriptionError::Download(format!("failed to run curl: {e}")))?;

    if !status.success() {
        return Err(TranscriptionError::Download(format!(
            "curl exited with status {status}"
        )));
    }
    Ok(())
}
