pub mod download;
pub mod manager;
pub mod model;
pub mod prompt;

pub use download::{download_model_if_needed, ModelSpec, PHI3_MINI_4BIT};
pub use manager::{LocalAIManager, SetupProgress};
pub use model::{
    check_memory, LocalAI, LocalAIConfig, LocalAIError, LocalAIResponse, LocalAIResult,
};
pub use prompt::SermonPromptBuilder;
