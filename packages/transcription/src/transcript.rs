// ─── TranscriptionSegment ─────────────────────────────────────────────────────

/// A single time-stamped piece of transcript produced by Whisper.
#[derive(Debug, Clone, PartialEq)]
pub struct TranscriptionSegment {
    /// Transcript text, whitespace-trimmed.
    pub text: String,

    /// Segment start in milliseconds from the beginning of the audio chunk.
    pub audio_start_ms: u64,

    /// Segment end in milliseconds.
    pub audio_end_ms: u64,

    /// Mean token probability across all tokens in this segment [0, 1].
    /// Lower values indicate Whisper was less certain about the transcription.
    pub whisper_confidence: f32,

    /// `true` when this segment's text was already emitted in a previous
    /// transcription window.  Set by the deduplication layer on top of this
    /// struct; always `false` when first produced by [`WhisperModel::transcribe`].
    pub is_duplicate: bool,

    /// Text of the neighbouring segments (previous + next) joined with a space.
    /// Gives the scripture-detection engine surrounding context to resolve
    /// ambiguous references like "verse 16" or "the third chapter".
    pub context_window: String,
}

impl TranscriptionSegment {
    /// Duration of this segment in milliseconds.
    pub fn duration_ms(&self) -> u64 {
        self.audio_end_ms.saturating_sub(self.audio_start_ms)
    }
}

// ─── TranscribeOptions ────────────────────────────────────────────────────────

/// Tuning knobs passed to [`WhisperModel::transcribe`].
#[derive(Debug, Clone)]
pub struct TranscribeOptions {
    /// BCP-47 language code for the audio (`"en"`, `"yo"` for Yoruba, etc.).
    /// `None` lets Whisper auto-detect — handles code-switching but is
    /// slightly slower.
    pub language: Option<String>,

    /// Prompt prepended to the decoder to bias Whisper toward domain
    /// vocabulary.  For church sermons a string like
    /// `"Scripture Bible verse chapter John Romans Genesis"` helps Whisper
    /// spell book names correctly and reduces hallucination on quiet passages.
    ///
    /// Leave empty (`String::new()`) to use no prompt.
    pub initial_prompt: String,

    /// Decoder temperature.  `0.0` is fully deterministic (greedy); higher
    /// values introduce randomness.  For transcription accuracy keep at `0.0`.
    pub temperature: f32,

    /// Number of CPU threads Whisper may use.  Defaults to 4.
    pub n_threads: i32,

    /// Segments whose no-speech probability exceeds this value are dropped.
    /// Range [0, 1].
    pub no_speech_threshold: f32,

    /// Maximum number of tokens per segment (`0` = no limit).
    pub max_tokens: i32,
}

impl Default for TranscribeOptions {
    fn default() -> Self {
        Self {
            language: Some("en".into()),
            initial_prompt: String::new(),
            temperature: 0.0,
            n_threads: 4,
            no_speech_threshold: 0.6,
            max_tokens: 0,
        }
    }
}

impl TranscribeOptions {
    /// Preset for Nigerian church sermons.
    ///
    /// - Language auto-detect handles Yoruba / Igbo / Pidgin code-switching.
    /// - Initial prompt seeds Whisper with common sermon vocabulary so book
    ///   names like "Ecclesiastes" or "Habakkuk" are spelled correctly.
    /// - Temperature 0.0 keeps output deterministic.
    pub fn church() -> Self {
        Self {
            language: None,
            initial_prompt: "Scripture Bible verse chapter Genesis Exodus Leviticus \
                Numbers Deuteronomy Joshua Judges Ruth Samuel Kings Chronicles \
                Ezra Nehemiah Esther Job Psalms Proverbs Ecclesiastes Isaiah \
                Jeremiah Lamentations Ezekiel Daniel Hosea Joel Amos Obadiah \
                Jonah Micah Nahum Habakkuk Zephaniah Haggai Zechariah Malachi \
                Matthew Mark Luke John Acts Romans Corinthians Galatians \
                Ephesians Philippians Colossians Thessalonians Timothy Titus \
                Philemon Hebrews James Peter Jude Revelation amen hallelujah \
                pastor sermon congregation church holy spirit"
                .into(),
            temperature: 0.0,
            n_threads: 4,
            no_speech_threshold: 0.5,
            max_tokens: 0,
        }
    }
}
