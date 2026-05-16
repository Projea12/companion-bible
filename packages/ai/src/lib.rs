pub mod download;
pub mod model;
pub mod prompt;

pub use download::{ModelSpec, PHI3_MINI_4BIT, download_model_if_needed};
pub use model::{LocalAI, LocalAIConfig, LocalAIError, LocalAIResponse, check_memory};
pub use prompt::SermonPromptBuilder;
