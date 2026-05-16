//! First-launch setup flow for the local Phi-3 model.

use std::path::{Path, PathBuf};

use crate::download::{download_model_if_needed, verify_sha256, PHI3_MINI_4BIT};
use crate::model::{LocalAI, LocalAIConfig, LocalAIError};

// ─── SetupProgress ────────────────────────────────────────────────────────────

/// Progress steps emitted by [`LocalAIManager::setup`].
#[derive(Debug, Clone)]
pub enum SetupProgress {
    /// Checking whether the model file already exists.
    Checking,
    /// Model is already present and the checksum matched.
    AlreadyPresent,
    /// Download in progress.
    Downloading { bytes_done: u64, bytes_total: Option<u64> },
    /// Verifying SHA-256 checksum.
    Verifying,
    /// Loading weights into memory.
    Loading,
    /// Model loaded and health check passed.
    Ready { model_path: PathBuf },
}

impl SetupProgress {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Checking => "Checking for Phi-3 model…",
            Self::AlreadyPresent => "Phi-3 model already downloaded",
            Self::Downloading { .. } => "Downloading Phi-3 model…",
            Self::Verifying => "Verifying checksum…",
            Self::Loading => "Loading Phi-3 into memory…",
            Self::Ready { .. } => "Phi-3 model ready",
        }
    }

    pub fn download_percent(&self) -> Option<u8> {
        match self {
            Self::Downloading { bytes_done, bytes_total: Some(total) } if *total > 0 => {
                Some(((*bytes_done as f64 / *total as f64) * 100.0).min(100.0) as u8)
            }
            _ => None,
        }
    }
}

// ─── LocalAIManager ───────────────────────────────────────────────────────────

/// Manages the first-launch download and load flow for the local Phi-3 model.
///
/// ## Usage
/// ```rust,ignore
/// let manager = LocalAIManager::new(&app_data_dir);
/// let ai = manager.setup(|progress| {
///     println!("{}", progress.label());
/// })?;
/// ```
///
/// `setup` is blocking — call it from `tokio::task::spawn_blocking` in async
/// contexts (e.g. a Tauri command).
pub struct LocalAIManager {
    models_dir: PathBuf,
}

impl LocalAIManager {
    /// Store model files under `app_data_dir/models/phi3/`.
    pub fn new(app_data_dir: &Path) -> Self {
        Self {
            models_dir: app_data_dir.join("models").join("phi3"),
        }
    }

    pub fn model_path(&self) -> PathBuf {
        self.models_dir.join(PHI3_MINI_4BIT.filename)
    }

    pub fn is_present(&self) -> bool {
        self.model_path().exists()
    }

    /// Full setup flow: download if needed → verify → load.
    ///
    /// `on_progress` is called at each stage and on every 64 KB chunk during
    /// the download.  Always called from the same thread as the caller.
    pub fn setup<F>(&self, mut on_progress: F) -> Result<LocalAI, LocalAIError>
    where
        F: FnMut(SetupProgress),
    {
        on_progress(SetupProgress::Checking);

        let path = self.model_path();

        if path.exists() {
            on_progress(SetupProgress::Verifying);
            verify_sha256(&path, PHI3_MINI_4BIT.sha256)
                .map_err(|e| LocalAIError::LoadFailed(e.to_string()))?;
            on_progress(SetupProgress::AlreadyPresent);
        } else {
            download_model_if_needed(&PHI3_MINI_4BIT, &self.models_dir, |bytes_done, bytes_total| {
                on_progress(SetupProgress::Downloading { bytes_done, bytes_total });
            })
            .map_err(|e| LocalAIError::LoadFailed(e.to_string()))?;

            on_progress(SetupProgress::Verifying);
            verify_sha256(&path, PHI3_MINI_4BIT.sha256)
                .map_err(|e| LocalAIError::LoadFailed(e.to_string()))?;
        }

        on_progress(SetupProgress::Loading);

        let config = LocalAIConfig::new(path.clone());
        let ai = LocalAI::load(config)?;

        on_progress(SetupProgress::Ready { model_path: path });

        Ok(ai)
    }
}
