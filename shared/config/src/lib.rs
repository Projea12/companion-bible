use serde::{Deserialize, Serialize};

pub mod audio {
    pub const SAMPLE_RATE: u32 = 16_000;
    pub const CHANNELS: u16 = 1;
    pub const CHUNK_DURATION_MS: u32 = 3_000;
    pub const BUFFER_SIZE: usize = 1_024;
    pub const MAX_SILENCE_MS: u32 = 1_500;
    pub const DEFAULT_DEVICE: &str = "default";
}

pub mod models {
    pub const WHISPER_DEFAULT: &str = "whisper-small";
    pub const WHISPER_LARGE: &str = "whisper-large-v3";
    pub const PHI3_DEFAULT: &str = "phi3-mini-4k-instruct";
    pub const MODELS_DIR: &str = "models";
}

pub mod database {
    pub const DB_FILE_NAME: &str = "companion.db";
    pub const SCHEMA_VERSION: u32 = 1;
    pub const BIBLE_FILE_NAME: &str = "bible.db";
}

pub mod app {
    pub const APP_NAME: &str = "Companion Bible";
    pub const APP_ID: &str = "com.companion-bible.app";
    pub const DEFAULT_TRANSLATION: &str = "ESV";
    pub const SUPPORTED_TRANSLATIONS: &[&str] = &["ESV", "KJV", "NIV", "NASB", "NLT", "CSB"];
    pub const UPDATE_CHECK_INTERVAL_SECS: u64 = 3_600;
    pub const WATCHDOG_INTERVAL_MS: u64 = 5_000;
    pub const MAX_COMPONENT_RESTARTS: u32 = 5;
}

pub mod api {
    pub const API_VERSION: &str = "v1";
    pub const REQUEST_TIMEOUT_MS: u64 = 30_000;
    pub const MAX_RETRIES: u32 = 3;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioConfig {
    pub device_id: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub chunk_duration_ms: u32,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            device_id: audio::DEFAULT_DEVICE.into(),
            sample_rate: audio::SAMPLE_RATE,
            channels: audio::CHANNELS,
            chunk_duration_ms: audio::CHUNK_DURATION_MS,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub whisper_model: String,
    pub phi3_model: String,
    pub models_dir: String,
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            whisper_model: models::WHISPER_DEFAULT.into(),
            phi3_model: models::PHI3_DEFAULT.into(),
            models_dir: models::MODELS_DIR.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub translation: String,
    pub audio: AudioConfig,
    pub models: ModelConfig,
    pub auto_display: bool,
    pub show_ai_context: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            translation: app::DEFAULT_TRANSLATION.into(),
            audio: AudioConfig::default(),
            models: ModelConfig::default(),
            auto_display: true,
            show_ai_context: true,
        }
    }
}
