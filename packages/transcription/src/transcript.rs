// ─── TranscriptionSegment ─────────────────────────────────────────────────────

/// A single time-stamped piece of transcript produced by Whisper.
#[derive(Debug, Clone, PartialEq)]
pub struct TranscriptionSegment {
    /// Transcript text, whitespace-trimmed.
    pub text: String,
    /// Segment start in milliseconds from the beginning of the audio chunk.
    pub start_ms: i64,
    /// Segment end in milliseconds.
    pub end_ms: i64,
}

impl TranscriptionSegment {
    /// Duration of this segment in milliseconds.
    pub fn duration_ms(&self) -> i64 {
        self.end_ms - self.start_ms
    }
}

// ─── TranscribeOptions ────────────────────────────────────────────────────────

/// Tuning knobs passed to [`WhisperModel::transcribe`].
#[derive(Debug, Clone)]
pub struct TranscribeOptions {
    /// BCP-47 language code for the audio (`"en"`, `"yo"` for Yoruba, etc.).
    /// `None` lets Whisper auto-detect the language — slightly slower.
    pub language: Option<String>,

    /// Number of CPU threads Whisper may use.  Defaults to 4; raise on
    /// machines with more cores to reduce latency.
    pub n_threads: i32,

    /// Segments whose no-speech probability exceeds this value are dropped.
    /// Range [0, 1]; `0.6` is a good starting point for church audio.
    pub no_speech_threshold: f32,

    /// Maximum number of tokens per segment (`0` = unlimited).
    pub max_tokens: i32,
}

impl Default for TranscribeOptions {
    fn default() -> Self {
        Self {
            language: Some("en".into()),
            n_threads: 4,
            no_speech_threshold: 0.6,
            max_tokens: 0,
        }
    }
}

impl TranscribeOptions {
    /// Options tuned for Nigerian church sermons (English with possible
    /// Yoruba/Igbo/Pidgin code-switching — let Whisper auto-detect).
    pub fn church() -> Self {
        Self {
            language: None, // auto-detect handles code-switching better
            n_threads: 4,
            no_speech_threshold: 0.5,
            max_tokens: 0,
        }
    }
}
