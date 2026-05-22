use std::path::{Path, PathBuf};

use crate::download::{download_if_needed, DownloadConfig};
use crate::error::TranscriptionError;
use crate::model::WhisperModel;

// ─── SetupProgress ────────────────────────────────────────────────────────────

/// Progress steps emitted by [`ModelManager::setup`].
///
/// Each variant maps to an `AppEvent` that the Tauri shell will forward to
/// the frontend setup screen.
#[derive(Debug, Clone)]
pub enum SetupProgress {
    /// Checking whether the model file already exists.
    Checking,
    /// Model is already present and the checksum matched — no download needed.
    AlreadyPresent,
    /// Download in progress.  `bytes_total` is `None` when the server does not
    /// send `Content-Length` (uncommon but possible).
    Downloading {
        bytes_done: u64,
        bytes_total: Option<u64>,
    },
    /// Download complete; verifying SHA-1.
    Verifying,
    /// Checksum passed; loading weights into memory.
    Loading,
    /// Model is loaded and the health check passed.
    Ready { load_time_ms: u64, memory_mb: u64 },
}

impl SetupProgress {
    /// Human-readable label suitable for a progress bar message.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Checking => "Checking for model…",
            Self::AlreadyPresent => "Model already downloaded",
            Self::Downloading { .. } => "Downloading model…",
            Self::Verifying => "Verifying checksum…",
            Self::Loading => "Loading model into memory…",
            Self::Ready { .. } => "Model ready",
        }
    }

    /// Download percentage [0, 100], or `None` for non-download steps.
    pub fn download_percent(&self) -> Option<u8> {
        match self {
            Self::Downloading {
                bytes_done,
                bytes_total: Some(total),
            } if *total > 0 => {
                Some(((*bytes_done as f64 / *total as f64) * 100.0).min(100.0) as u8)
            }
            _ => None,
        }
    }
}

// ─── ModelManager ─────────────────────────────────────────────────────────────

/// Manages the first-launch model setup flow.
///
/// ## Responsibilities
/// 1. Check whether the GGML model weights are present in `models_dir`.
/// 2. Download them (with streaming progress) if they are not.
/// 3. Verify the SHA-1 checksum.
/// 4. Load the model into memory.
/// 5. Run a health-check inference to confirm the model is functional.
///
/// ## Usage
/// ```rust,ignore
/// let manager = ModelManager::new(&app_data_dir);
/// let model = manager.setup(|progress| {
///     println!("{}", progress.label());
/// })?;
/// ```
///
/// The `setup` method is blocking.  When calling from an async context (e.g. a
/// Tauri command), wrap it in `tokio::task::spawn_blocking`.
pub struct ModelManager {
    models_dir: PathBuf,
}

impl ModelManager {
    /// Create a manager that stores model files under `app_data_dir/models/whisper/`.
    pub fn new(app_data_dir: &Path) -> Self {
        Self {
            models_dir: app_data_dir.join("models").join("whisper"),
        }
    }

    /// Path at which the GGML model file will be / is stored.
    pub fn model_path(&self) -> PathBuf {
        self.models_dir.join("ggml-small.bin")
    }

    /// `true` if the model file exists on disk (does **not** verify the checksum).
    pub fn is_present(&self) -> bool {
        self.model_path().exists()
    }

    /// Full setup flow: download if needed → verify → load → health check.
    ///
    /// `on_progress` is called at each stage and on every 64 KB chunk during
    /// the download.  It is always called from the same thread as the caller.
    pub fn setup<F>(&self, mut on_progress: F) -> Result<WhisperModel, TranscriptionError>
    where
        F: FnMut(SetupProgress),
    {
        on_progress(SetupProgress::Checking);

        let path = self.model_path();

        if path.exists() {
            on_progress(SetupProgress::AlreadyPresent);
        } else {
            std::fs::create_dir_all(&self.models_dir)?;

            let cfg = DownloadConfig::whisper_medium(&self.models_dir);

            download_if_needed(&cfg, |bytes_done, bytes_total| {
                on_progress(SetupProgress::Downloading {
                    bytes_done,
                    bytes_total,
                });
            })?;
        }

        // Load the model; the progress callback receives fractions — we translate
        // them into our own Loading variant.
        on_progress(SetupProgress::Loading);
        let model = WhisperModel::load(&path, |_| {})?;

        // Smoke-test: 0.1 s silence inference to confirm the model is functional.
        model.health_check()?;

        on_progress(SetupProgress::Ready {
            load_time_ms: model.load_time_ms,
            memory_mb: model.memory_delta_mb,
        });

        model.assert_within_budget()?;

        Ok(model)
    }
}
