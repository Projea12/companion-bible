use thiserror::Error;

#[derive(Debug, Error)]
pub enum AudioError {
    #[error("audio device not found: {device_id}")]
    DeviceNotFound { device_id: String },
    #[error("failed to open audio stream: {reason}")]
    StreamOpenFailed { reason: String },
    #[error("audio capture interrupted: {reason}")]
    CaptureInterrupted { reason: String },
    #[error("unsupported sample rate: {rate}Hz")]
    UnsupportedSampleRate { rate: u32 },
    #[error("buffer overflow after {dropped} frames dropped")]
    BufferOverflow { dropped: u64 },
}

#[derive(Debug, Error)]
pub enum TranscriptionError {
    #[error("model not loaded: {model}")]
    ModelNotLoaded { model: String },
    #[error("model inference failed: {reason}")]
    InferenceFailed { reason: String },
    #[error("audio chunk {chunk_id} too short to transcribe ({duration_ms}ms)")]
    ChunkTooShort { chunk_id: u64, duration_ms: u32 },
    #[error("transcription timed out after {timeout_ms}ms")]
    Timeout { timeout_ms: u32 },
    #[error("transcription worker crashed: {reason}")]
    WorkerCrashed { reason: String },
}

#[derive(Debug, Error)]
pub enum DetectionError {
    #[error("detection model not ready")]
    ModelNotReady,
    #[error("input text too long: {length} chars (max {max})")]
    InputTooLong { length: usize, max: usize },
    #[error("detection failed: {reason}")]
    DetectionFailed { reason: String },
}

#[derive(Debug, Error)]
pub enum BibleError {
    #[error("translation '{translation}' not installed")]
    TranslationNotInstalled { translation: String },
    #[error("book '{book}' not found")]
    BookNotFound { book: String },
    #[error("{book} has only {total} chapters, requested chapter {requested}")]
    ChapterOutOfRange {
        book: String,
        requested: u8,
        total: u8,
    },
    #[error("{book} {chapter} has only {total} verses, requested verse {requested}")]
    VerseOutOfRange {
        book: String,
        chapter: u8,
        requested: u8,
        total: u8,
    },
    #[error("database query failed: {reason}")]
    QueryFailed { reason: String },
}

#[derive(Debug, Error)]
pub enum AiError {
    #[error("model '{model}' not loaded")]
    ModelNotLoaded { model: String },
    #[error("context window exceeded: {tokens} tokens (max {max})")]
    ContextWindowExceeded { tokens: usize, max: usize },
    #[error("inference failed: {reason}")]
    InferenceFailed { reason: String },
    #[error("AI request timed out after {timeout_ms}ms")]
    Timeout { timeout_ms: u32 },
    #[error("AI worker is busy")]
    WorkerBusy,
}

#[derive(Debug, Error)]
pub enum DatabaseError {
    #[error("database file not found at: {path}")]
    FileNotFound { path: String },
    #[error("migration from v{from} to v{to} failed: {reason}")]
    MigrationFailed { from: u32, to: u32, reason: String },
    #[error("query failed: {reason}")]
    QueryFailed { reason: String },
    #[error("database is locked")]
    Locked,
    #[error("schema version mismatch: expected v{expected}, found v{found}")]
    SchemaMismatch { expected: u32, found: u32 },
}

#[derive(Debug, Error)]
pub enum DisplayError {
    #[error("display window not available")]
    WindowNotAvailable,
    #[error("render failed: {reason}")]
    RenderFailed { reason: String },
    #[error("font not loaded: {font}")]
    FontNotLoaded { font: String },
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("config file not found at: {path}")]
    FileNotFound { path: String },
    #[error("config parse error: {reason}")]
    ParseFailed { reason: String },
    #[error("unknown config key: '{key}'")]
    UnknownKey { key: String },
    #[error("invalid value for '{key}': {reason}")]
    InvalidValue { key: String, reason: String },
}

#[derive(Debug, Error)]
pub enum UpdateError {
    #[error("update server unreachable: {reason}")]
    ServerUnreachable { reason: String },
    #[error("download failed for version {version}: {reason}")]
    DownloadFailed { version: String, reason: String },
    #[error("signature verification failed for version {version}")]
    SignatureInvalid { version: String },
    #[error("install failed for version {version}: {reason}")]
    InstallFailed { version: String, reason: String },
}

#[derive(Debug, Error)]
pub enum WatchdogError {
    #[error("component '{component}' failed to start: {reason}")]
    StartFailed { component: String, reason: String },
    #[error("component '{component}' exceeded max restarts ({max})")]
    MaxRestartsExceeded { component: String, max: u32 },
    #[error("health check timed out for '{component}'")]
    HealthCheckTimeout { component: String },
}

#[derive(Debug, Error)]
pub enum AppError {
    #[error(transparent)]
    Audio(#[from] AudioError),
    #[error(transparent)]
    Transcription(#[from] TranscriptionError),
    #[error(transparent)]
    Detection(#[from] DetectionError),
    #[error(transparent)]
    Bible(#[from] BibleError),
    #[error(transparent)]
    Ai(#[from] AiError),
    #[error(transparent)]
    Database(#[from] DatabaseError),
    #[error(transparent)]
    Display(#[from] DisplayError),
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Update(#[from] UpdateError),
    #[error(transparent)]
    Watchdog(#[from] WatchdogError),
}
