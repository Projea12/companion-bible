use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

// ─── Error ────────────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum WalError {
    #[error("WAL I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("WAL serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

// ─── Entry ────────────────────────────────────────────────────────────────────

/// One entry per meaningful state change in the application.
/// Used for crash recovery — on restart the log can be replayed to restore
/// any writes that were flushed to WAL but not yet committed to SQLite.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WalEntry {
    /// The church has been registered for the first time (onboarding).
    ChurchRegistered {
        church_id: String,
        name: String,
        region: String,
    },
    /// A new sermon session has begun.
    SermonStarted {
        sermon_id: String,
        church_id: String,
        started_at: String,
    },
    /// A running sermon session has ended.
    SermonEnded {
        sermon_id: String,
        ended_at: String,
    },
    /// The pipeline produced a detection result for a transcript chunk.
    DetectionRecorded {
        event_id: String,
        sermon_id: String,
        raw_transcript: String,
        final_reference: Option<String>,
        confidence: f64,
        decision: String,
        processing_time_ms: i64,
    },
    /// An operator has manually accepted, rejected, or corrected a detection.
    OperatorCorrected {
        event_id: String,
        action: String,
        correct_reference: Option<String>,
    },
    /// Calibration thresholds for a pipeline stage have been updated.
    CalibrationUpdated {
        church_id: String,
        stage: String,
        accept_above: f64,
        escalate_below: f64,
    },
    /// A church-level key/value setting has been changed.
    SettingChanged {
        key: String,
        value: String,
    },
    /// End-of-sermon aggregate statistics have been saved.
    ServiceRecordSaved {
        record_id: String,
        sermon_id: String,
        total_detections: i64,
        auto_accepted: i64,
        operator_corrected: i64,
        rejected: i64,
    },
}

// ─── On-disk record ───────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize)]
struct WalRecord {
    seq: u64,
    /// Unix timestamp (seconds) at time of write.
    ts: u64,
    entry: WalEntry,
}

// ─── WriteAheadLog ────────────────────────────────────────────────────────────

struct WalInner {
    file: File,
    sequence: u64,
}

/// Append-only, fsync-on-write log stored as newline-delimited JSON.
///
/// Each call to [`write`] increments the sequence counter, serialises the
/// entry, appends it as a single JSON line, and calls `sync_all` before
/// returning.  The sequence counter resumes from the number of existing
/// entries when the file is opened, so it is stable across restarts.
pub struct WriteAheadLog {
    inner: Mutex<WalInner>,
}

impl WriteAheadLog {
    /// Open (or create) the WAL file at `path`.
    ///
    /// Parent directories are created automatically.  If the file already
    /// exists its current entry count becomes the starting sequence number so
    /// new writes continue from where the last session left off.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, WalError> {
        let path = path.into();

        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }

        let sequence = if path.exists() {
            count_entries(&path)?
        } else {
            0
        };

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;

        Ok(Self {
            inner: Mutex::new(WalInner { file, sequence }),
        })
    }

    /// Serialize `entry`, append it to the log, sync to disk, and return the
    /// sequence number assigned to this entry.
    pub fn write(&self, entry: WalEntry) -> Result<u64, WalError> {
        let mut inner = self.inner.lock().expect("WAL mutex poisoned");

        inner.sequence += 1;
        let seq = inner.sequence;

        let record = WalRecord {
            seq,
            ts: unix_now(),
            entry,
        };

        let mut line = serde_json::to_string(&record)?;
        line.push('\n');

        inner.file.write_all(line.as_bytes())?;
        inner.file.sync_all()?;

        Ok(seq)
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn count_entries(path: &Path) -> Result<u64, WalError> {
    let file = File::open(path)?;
    let count = BufReader::new(file)
        .lines()
        .filter(|l| l.as_ref().map(|s| !s.trim().is_empty()).unwrap_or(false))
        .count();
    Ok(count as u64)
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
