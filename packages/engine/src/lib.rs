mod decision;
mod engine;
mod fuzzy;
mod layers;
mod quotation;
mod worker;

pub use decision::{DetectionDecision, ValidationOutcome};
pub use engine::{DetectionEngine, EngineConfig};
pub use worker::LocalAiHandle;
