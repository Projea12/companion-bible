use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};

use sha1::{Digest, Sha1};

use crate::error::TranscriptionError;
use crate::model::{GGML_MEDIUM_SHA1, GGML_MEDIUM_URL};

// ─── DownloadConfig ───────────────────────────────────────────────────────────

/// Where to get a model file and how to verify it.
pub struct DownloadConfig {
    /// HTTP(S) source URL.
    pub url: String,
    /// Expected SHA-1 hex digest (lowercase, 40 chars).
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

/// Download the model if it is not already present, then verify checksum.
///
/// `on_progress(bytes_done, bytes_total)` is called on every 64 KB chunk.
/// `bytes_total` is `None` when the server does not send `Content-Length`.
///
/// A `.tmp` suffix is used during transfer; the file is renamed to `dest` only
/// after checksum passes.  An existing file whose checksum matches is accepted
/// without re-downloading.
pub fn download_if_needed<F>(
    cfg: &DownloadConfig,
    mut on_progress: F,
) -> Result<(), TranscriptionError>
where
    F: FnMut(u64, Option<u64>),
{
    if cfg.dest.exists() {
        // Signal that we're verifying an existing file.
        on_progress(0, None);
        verify_sha1(&cfg.dest, &cfg.sha1)?;
        return Ok(());
    }

    if let Some(parent) = cfg.dest.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let tmp = cfg.dest.with_extension("tmp");

    fetch_with_progress(&cfg.url, &tmp, &mut on_progress)?;

    verify_sha1(&tmp, &cfg.sha1).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        e
    })?;

    std::fs::rename(&tmp, &cfg.dest)?;
    Ok(())
}

// ─── verify_sha1 ─────────────────────────────────────────────────────────────

/// Stream `path` through SHA-1 and verify it matches `expected` (hex, any case).
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

/// Stream `url` into `dest`, calling `on_progress(bytes_done, bytes_total)` on
/// every 64 KB chunk.  Uses `reqwest` blocking client so no async runtime is
/// required — call from a `tokio::task::spawn_blocking` context when needed.
fn fetch_with_progress<F>(
    url: &str,
    dest: &Path,
    on_progress: &mut F,
) -> Result<(), TranscriptionError>
where
    F: FnMut(u64, Option<u64>),
{
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(3_600)) // 1 h — large file
        .build()
        .map_err(|e| TranscriptionError::Download(e.to_string()))?;

    let mut response = client
        .get(url)
        .send()
        .map_err(|e| TranscriptionError::Download(e.to_string()))?;

    if !response.status().is_success() {
        return Err(TranscriptionError::Download(format!(
            "HTTP {} for {url}",
            response.status()
        )));
    }

    let total = response.content_length();
    let mut file = std::fs::File::create(dest)?;
    let mut downloaded: u64 = 0;
    let mut buf = vec![0u8; 65_536];

    loop {
        let n = response
            .read(&mut buf)
            .map_err(|e| TranscriptionError::Download(e.to_string()))?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])?;
        downloaded += n as u64;
        on_progress(downloaded, total);
    }

    Ok(())
}
