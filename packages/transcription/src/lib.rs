mod error;
pub mod download;
pub mod manager;
pub mod model;

pub use error::TranscriptionError;
pub use download::{download_if_needed, verify_sha1, DownloadConfig};
pub use manager::{ModelManager, SetupProgress};
pub use model::{
    rss_mb, HealthReport, WhisperModel, GGML_MEDIUM_SHA1, GGML_MEDIUM_URL,
    MEMORY_BUDGET_MB,
};

#[cfg(test)]
mod tests;
