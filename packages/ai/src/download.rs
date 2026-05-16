//! Model download and checksum verification for the AI layer.

use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use thiserror::Error;

// ─── Constants ────────────────────────────────────────────────────────────────

pub const PHI3_MINI_URL: &str =
    "https://huggingface.co/microsoft/Phi-3-mini-4k-instruct-gguf/resolve/main/Phi-3-mini-4k-instruct-q4.gguf";

/// Expected SHA-256 digest for `Phi-3-mini-4k-instruct-q4.gguf`.
pub const PHI3_MINI_SHA256: &str =
    "8a83c7fb9049a9b2e92266fa7ad04933bb53aa1e85136b7b30f1b8000ff2edef";

/// Approximate model size in MB (~2.39 GB Q4 quantised).
pub const PHI3_MINI_SIZE_MB: u64 = 2_283;

// ─── ModelSpec ────────────────────────────────────────────────────────────────

/// Everything needed to locate, fetch, and verify a GGUF model file.
pub struct ModelSpec {
    pub url: &'static str,
    pub sha256: &'static str,
    pub size_mb: u64,
    pub filename: &'static str,
}

pub const PHI3_MINI_4BIT: ModelSpec = ModelSpec {
    url: PHI3_MINI_URL,
    sha256: PHI3_MINI_SHA256,
    size_mb: PHI3_MINI_SIZE_MB,
    filename: "Phi-3-mini-4k-instruct-q4.gguf",
};

// ─── DownloadError ────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum DownloadError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("download failed: {0}")]
    Http(String),

    #[error("checksum mismatch — expected {expected}, got {actual}")]
    ChecksumMismatch { expected: String, actual: String },
}

// ─── download_model_if_needed ─────────────────────────────────────────────────

/// Download `spec` into `models_dir` if not already present and verified.
///
/// `on_progress(bytes_done, bytes_total)` is called on every 64 KB chunk.
/// Uses a `.tmp` staging file; only renamed to final path after checksum passes.
pub fn download_model_if_needed<F>(
    spec: &ModelSpec,
    models_dir: &Path,
    mut on_progress: F,
) -> Result<PathBuf, DownloadError>
where
    F: FnMut(u64, Option<u64>),
{
    let dest = models_dir.join(spec.filename);

    if dest.exists() {
        on_progress(0, None);
        verify_sha256(&dest, spec.sha256)?;
        return Ok(dest);
    }

    std::fs::create_dir_all(models_dir)?;

    let tmp = dest.with_extension("tmp");
    fetch_with_progress(spec.url, &tmp, &mut on_progress, spec.size_mb * 1_048_576)?;

    verify_sha256(&tmp, spec.sha256).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        e
    })?;

    std::fs::rename(&tmp, &dest)?;
    Ok(dest)
}

// ─── verify_sha256 ────────────────────────────────────────────────────────────

pub fn verify_sha256(path: &Path, expected: &str) -> Result<(), DownloadError> {
    let file = std::fs::File::open(path)?;
    let mut reader = std::io::BufReader::new(file);
    let mut hasher = Sha256::new();
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
        return Err(DownloadError::ChecksumMismatch {
            expected: expected.to_string(),
            actual,
        });
    }
    Ok(())
}

// ─── fetch_with_progress ──────────────────────────────────────────────────────

/// Download `url` into `dest`, resuming from an existing partial `.tmp` file
/// if present.  `total_hint` is used when the server omits `Content-Length`.
fn fetch_with_progress<F>(
    url: &str,
    dest: &Path,
    on_progress: &mut F,
    total_hint: u64,
) -> Result<(), DownloadError>
where
    F: FnMut(u64, Option<u64>),
{
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(7_200))
        .build()
        .map_err(|e| DownloadError::Http(e.to_string()))?;

    // Check for a partial download to resume from.
    let already = if dest.exists() {
        std::fs::metadata(dest).map(|m| m.len()).unwrap_or(0)
    } else {
        0
    };

    let mut request = client.get(url);
    if already > 0 {
        request = request.header("Range", format!("bytes={already}-"));
    }

    let mut response = request
        .send()
        .map_err(|e| DownloadError::Http(e.to_string()))?;

    let status = response.status();
    // 200 OK (full) or 206 Partial Content (resume) are both valid.
    if !status.is_success() {
        return Err(DownloadError::Http(format!("HTTP {status} for {url}")));
    }

    // If the server honoured the Range request we append; otherwise restart.
    let (mut file, mut downloaded) = if status == reqwest::StatusCode::PARTIAL_CONTENT && already > 0 {
        let f = std::fs::OpenOptions::new().append(true).open(dest)?;
        (f, already)
    } else {
        (std::fs::File::create(dest)?, 0)
    };

    let total = response.content_length().map(|n| n + downloaded).or(if total_hint > 0 { Some(total_hint) } else { None });
    let mut buf = vec![0u8; 65_536];

    loop {
        let n = response
            .read(&mut buf)
            .map_err(|e| DownloadError::Http(e.to_string()))?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])?;
        downloaded += n as u64;
        on_progress(downloaded, total);
    }

    Ok(())
}
