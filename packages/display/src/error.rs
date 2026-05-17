/// Errors that `DisplayController` operations can produce.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum DisplayError {
    #[error("WAL write failed: {0}")]
    WalWriteFailed(String),

    #[error("render failed: {0}")]
    RenderFailed(String),

    #[error("no history to discard")]
    NoHistory,
}
