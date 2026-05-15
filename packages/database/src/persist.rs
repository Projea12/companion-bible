use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::wal::AppState;

// ─── Error ────────────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum PersistError {
    #[error("state I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("state serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

// ─── AppStateSerializer ───────────────────────────────────────────────────────

/// Persists `AppState` to disk and manages the crash-detection marker file.
///
/// Startup decision flow:
/// ```text
/// if crash_marker_exists()  →  recover from WAL (app crashed last time)
/// else                       →  load_state()     (clean shutdown last time)
/// write_crash_marker()
/// … run app …
/// save_state(&current_state)
/// delete_crash_marker()
/// ```
///
/// Both the state file and the marker live under the same directory, which
/// should be the app's per-user data directory.
pub struct AppStateSerializer {
    /// `{dir}/app_state.json`
    state_path: PathBuf,
    /// `{dir}/crash.marker`
    marker_path: PathBuf,
}

impl AppStateSerializer {
    /// Create a serializer rooted at `dir`.
    ///
    /// The directory is created automatically on first `save_state` call;
    /// it does not need to exist when the serializer is constructed.
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        let dir = dir.into();
        Self {
            state_path: dir.join("app_state.json"),
            marker_path: dir.join("crash.marker"),
        }
    }

    /// Path of the state file (useful for tests and diagnostics).
    pub fn state_path(&self) -> &Path {
        &self.state_path
    }

    /// Path of the crash marker file (useful for tests and diagnostics).
    pub fn marker_path(&self) -> &Path {
        &self.marker_path
    }

    // ── State ─────────────────────────────────────────────────────────────────

    /// Serialize `state` to the state file.
    ///
    /// Uses an atomic write-then-rename so a crash during saving never leaves
    /// a partially-written file.  The file is fsynced before rename.
    pub fn save_state(&self, state: &AppState) -> Result<(), PersistError> {
        if let Some(parent) = self.state_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let json = serde_json::to_vec_pretty(state)?;
        let tmp = self.state_path.with_extension("tmp");

        let mut file = File::create(&tmp)?;
        file.write_all(&json)?;
        file.sync_all()?;
        drop(file);

        std::fs::rename(&tmp, &self.state_path)?;
        Ok(())
    }

    /// Deserialize `AppState` from the state file.
    ///
    /// Returns `Ok(None)` if the file does not exist or its contents are
    /// corrupt.  Returns `Err` only for unrecoverable I/O errors (e.g.
    /// permission denied).
    pub fn load_state(&self) -> Result<Option<AppState>, PersistError> {
        let data = match std::fs::read(&self.state_path) {
            Ok(d) => d,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };

        match serde_json::from_slice::<AppState>(&data) {
            Ok(state) => Ok(Some(state)),
            Err(e) => {
                eprintln!(
                    "state file at {:?} is corrupted and will be ignored: {e}",
                    self.state_path
                );
                Ok(None)
            }
        }
    }

    // ── Crash marker ──────────────────────────────────────────────────────────

    /// Write the crash marker.
    ///
    /// Call this at startup, before any mutable operations, so a subsequent
    /// crash leaves the marker in place for the next launch to detect.
    pub fn write_crash_marker(&self) -> Result<(), PersistError> {
        if let Some(parent) = self.marker_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&self.marker_path, b"running\n")?;
        Ok(())
    }

    /// Delete the crash marker.
    ///
    /// Call this immediately before shutting down cleanly.  It is not an
    /// error if the marker does not exist.
    pub fn delete_crash_marker(&self) -> Result<(), PersistError> {
        match std::fs::remove_file(&self.marker_path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    /// Return `true` if the crash marker file exists.
    ///
    /// A present marker means the previous session ended abnormally; the
    /// caller should recover state from the WAL instead of the state file.
    pub fn crash_marker_exists(&self) -> bool {
        self.marker_path.exists()
    }
}
