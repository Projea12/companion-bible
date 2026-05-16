use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TranscriptionError {
    #[error("model file not found: {0}")]
    ModelNotFound(PathBuf),

    #[error("invalid model path (non-UTF-8)")]
    InvalidPath,

    #[error("checksum mismatch — expected {expected}, got {actual}")]
    ChecksumMismatch { expected: String, actual: String },

    #[error("whisper error: {0}")]
    Whisper(#[from] whisper_rs::WhisperError),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("download error: {0}")]
    Download(String),

    #[error("health check failed: {0}")]
    HealthCheck(String),

    #[error("transcription failed: {0}")]
    Transcribe(String),
}
