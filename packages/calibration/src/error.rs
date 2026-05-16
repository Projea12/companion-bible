use thiserror::Error;

#[derive(Debug, Error)]
pub enum CalibrationError {
    #[error("database error: {0}")]
    Database(#[from] companion_errors::DatabaseError),

    #[error("persisted threshold value could not be parsed: {0}")]
    ParseError(String),

    #[error("no church record found — run onboarding first")]
    NoChurch,
}
