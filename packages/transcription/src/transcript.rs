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

// ─── Prompt constants ─────────────────────────────────────────────────────────

/// Opening sentence that frames the audio for Whisper's decoder.
///
/// Placed at the start of every initial prompt so Whisper expects scripture
/// vocabulary, proper nouns, and the spoken cadence of a sermon.
pub const SERMON_PREAMBLE: &str = "A preacher is delivering a sermon from the Bible. \
     Scripture references include book names, chapter numbers, and verse numbers.";

/// All 66 canonical Bible book names, space-separated.
///
/// Injected into the initial prompt so Whisper learns the correct spellings of
/// uncommon names like Habakkuk, Ecclesiastes, and Zephaniah.
pub const BIBLE_BOOKS: &str = "Genesis Exodus Leviticus Numbers Deuteronomy Joshua Judges Ruth \
     Samuel Kings Chronicles Ezra Nehemiah Esther Job Psalms Proverbs \
     Ecclesiastes Isaiah Jeremiah Lamentations Ezekiel Daniel Hosea Joel \
     Amos Obadiah Jonah Micah Nahum Habakkuk Zephaniah Haggai Zechariah \
     Malachi Matthew Mark Luke John Acts Romans Corinthians Galatians \
     Ephesians Philippians Colossians Thessalonians Timothy Titus Philemon \
     Hebrews James Peter Jude Revelation";

/// Church-specific vocabulary appended after the book list.
const CHURCH_VOCAB: &str =
    "amen hallelujah pastor sermon congregation church holy spirit verse chapter";

// ─── TranscribeOptions ────────────────────────────────────────────────────────

/// Tuning knobs passed to [`WhisperModel::transcribe`].
#[derive(Debug, Clone)]
pub struct TranscribeOptions {
    /// BCP-47 language code for the audio (`"en"`, `"yo"` for Yoruba, etc.).
    /// `None` lets Whisper auto-detect — handles code-switching but is
    /// slightly slower.
    pub language: Option<String>,

    /// Prompt prepended to the decoder to bias Whisper toward domain
    /// vocabulary.  Build with [`TranscribeOptions::build_prompt`] rather
    /// than constructing manually.
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
    // ── Prompt construction ───────────────────────────────────────────────────

    /// Build an initial prompt for Whisper.
    ///
    /// The prompt always opens with [`SERMON_PREAMBLE`] and includes all 66
    /// Bible book names.  When `book` is `Some`, the current book is called
    /// out explicitly so Whisper can resolve bare references like "chapter 3
    /// verse 16" without the book name being spoken.
    ///
    /// ```rust
    /// use companion_transcription::TranscribeOptions;
    ///
    /// let generic = TranscribeOptions::build_prompt(None);
    /// assert!(generic.contains("A preacher is delivering a sermon"));
    ///
    /// let contextual = TranscribeOptions::build_prompt(Some("Romans"));
    /// assert!(contextual.contains("The current passage is from the book of Romans"));
    /// ```
    pub fn build_prompt(book: Option<&str>) -> String {
        match book {
            Some(b) => format!(
                "{SERMON_PREAMBLE} The current passage is from the book of {b}. \
                 {BIBLE_BOOKS} {CHURCH_VOCAB}"
            ),
            None => format!("{SERMON_PREAMBLE} {BIBLE_BOOKS} {CHURCH_VOCAB}"),
        }
    }

    // ── Presets ───────────────────────────────────────────────────────────────

    /// Preset for Nigerian church sermons with optional detected book context.
    ///
    /// - Language auto-detect handles Yoruba / Igbo / Pidgin code-switching.
    /// - Initial prompt is built by [`build_prompt`] with the supplied book.
    /// - Temperature 0.0 keeps output deterministic.
    ///
    /// The `WhisperTranscriber` calls this every run, passing the most recently
    /// identified book from the scripture-detection layer, so Whisper gets
    /// progressively better at resolving ambiguous references mid-sermon.
    pub fn with_context(book: Option<&str>) -> Self {
        Self {
            language: None,
            initial_prompt: Self::build_prompt(book),
            temperature: 0.0,
            n_threads: 4,
            no_speech_threshold: 0.5,
            max_tokens: 0,
        }
    }

    /// Preset for Nigerian church sermons with no book context (generic prompt).
    ///
    /// Equivalent to `TranscribeOptions::with_context(None)`.
    pub fn church() -> Self {
        Self::with_context(None)
    }
}
