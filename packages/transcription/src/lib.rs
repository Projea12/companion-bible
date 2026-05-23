pub mod assemblyai;
pub mod channel;
pub mod correction;
pub mod deepgram;
pub mod download;
mod error;
pub mod transcript;

// Whisper local inference is not available on Windows due to whisper.cpp/MSVC
// ABI incompatibility. These modules compile only on macOS and Linux.
#[cfg(not(target_os = "windows"))]
pub mod manager;
#[cfg(not(target_os = "windows"))]
pub mod model;
#[cfg(not(target_os = "windows"))]
pub mod transcriber;

pub use assemblyai::AssemblyAiTranscriber;
pub use channel::{
    segment_channel, segment_channel_with_capacity, SegmentReceiver, SegmentSender,
    CHANNEL_CAPACITY,
};
pub use correction::{correct_batch, correct_segment, correct_text, CORRECTIONS};
pub use deepgram::DeepgramTranscriber;
pub use download::{download_if_needed, verify_sha1, DownloadConfig};
pub use error::TranscriptionError;
pub use transcript::{TranscribeOptions, TranscriptionSegment, BIBLE_BOOKS, SERMON_PREAMBLE};

#[cfg(not(target_os = "windows"))]
pub use manager::{ModelManager, SetupProgress};
#[cfg(not(target_os = "windows"))]
pub use model::{
    rss_mb, HealthReport, WhisperModel, GGML_MEDIUM_SHA1, GGML_MEDIUM_URL, MEMORY_BUDGET_MB,
};
#[cfg(not(target_os = "windows"))]
pub use transcriber::{WhisperTranscriber, NEW_AUDIO_SECS, TRANSCRIBE_WINDOW_SECS};

#[cfg(test)]
mod tests;
