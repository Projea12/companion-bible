pub mod assemblyai;
pub mod channel;
pub mod correction;
pub mod deepgram;
pub mod download;
mod error;
pub mod manager;
pub mod model;
pub mod transcriber;
pub mod transcript;

pub use assemblyai::AssemblyAiTranscriber;
pub use channel::{
    segment_channel, segment_channel_with_capacity, SegmentReceiver, SegmentSender,
    CHANNEL_CAPACITY,
};
pub use correction::{correct_batch, correct_segment, correct_text, CORRECTIONS};
pub use deepgram::DeepgramTranscriber;
pub use download::{download_if_needed, verify_sha1, DownloadConfig};
pub use error::TranscriptionError;
pub use manager::{ModelManager, SetupProgress};
pub use model::{
    rss_mb, HealthReport, WhisperModel, GGML_SMALL_SHA1, GGML_SMALL_URL, MEMORY_BUDGET_MB,
};
pub use transcriber::{WhisperTranscriber, NEW_AUDIO_SECS, TRANSCRIBE_WINDOW_SECS};
pub use transcript::{TranscribeOptions, TranscriptionSegment, BIBLE_BOOKS, SERMON_PREAMBLE};

#[cfg(test)]
mod tests;
