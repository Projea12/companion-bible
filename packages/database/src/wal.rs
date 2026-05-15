use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::models::{CalibrationThresholds, Church, DetectionEvent, Sermon};

// ─── Error ────────────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum WalError {
    #[error("WAL I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("WAL serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

// ─── AppState ─────────────────────────────────────────────────────────────────

/// Full snapshot of recoverable application state.
/// Written by `checkpoint()` and reconstructed by `replay()`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct AppState {
    /// The church registered on this device.
    pub church: Option<Church>,
    /// The sermon currently in progress, if any.
    pub active_sermon: Option<Sermon>,
    /// Detection events from the current sermon not yet committed to SQLite.
    pub pending_detections: Vec<DetectionEvent>,
    /// Church settings cache (key → JSON value string).
    pub settings: HashMap<String, String>,
    /// Calibration thresholds for all pipeline stages.
    pub calibration: Vec<CalibrationThresholds>,
}

// ─── Entry ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WalEntry {
    ChurchRegistered {
        church_id: String,
        name: String,
        region: String,
    },
    SermonStarted {
        sermon_id: String,
        church_id: String,
        started_at: String,
    },
    SermonEnded {
        sermon_id: String,
        ended_at: String,
    },
    DetectionRecorded {
        event_id: String,
        sermon_id: String,
        raw_transcript: String,
        final_reference: Option<String>,
        confidence: f64,
        decision: String,
        processing_time_ms: i64,
    },
    OperatorCorrected {
        event_id: String,
        action: String,
        correct_reference: Option<String>,
    },
    CalibrationUpdated {
        church_id: String,
        stage: String,
        accept_above: f64,
        escalate_below: f64,
    },
    SettingChanged {
        key: String,
        value: String,
    },
    ServiceRecordSaved {
        record_id: String,
        sermon_id: String,
        total_detections: i64,
        auto_accepted: i64,
        operator_corrected: i64,
        rejected: i64,
    },
    /// Full state snapshot. Used as the starting point during `replay()`.
    /// Written every second during an active sermon.
    Checkpoint { state: AppState },
}

// ─── On-disk record ───────────────────────────────────────────────────────────

/// One line in the WAL file.
/// `crc` is the FNV-1a 64-bit hash of the JSON-serialized `entry` field.
#[derive(Serialize, Deserialize)]
struct WalRecord {
    seq: u64,
    ts: u64,
    crc: u64,
    entry: WalEntry,
}

// ─── WriteAheadLog ────────────────────────────────────────────────────────────

struct WalInner {
    file: File,
    sequence: u64,
}

/// Append-only, fsync-on-write log stored as newline-delimited JSON.
///
/// Every record includes a FNV-1a checksum of its entry.  `replay()` verifies
/// each checksum and silently skips any line that fails, making the log
/// resilient to partial writes and bit-rot.
pub struct WriteAheadLog {
    path: PathBuf,
    inner: Mutex<WalInner>,
}

impl WriteAheadLog {
    /// Open (or create) the WAL file at `path`.
    ///
    /// Parent directories are created automatically.  If the file already
    /// exists the sequence counter resumes from its current entry count.
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
            path,
            inner: Mutex::new(WalInner { file, sequence }),
        })
    }

    /// Serialize `entry`, compute its checksum, append it to the log,
    /// sync to disk, and return the sequence number assigned to it.
    pub fn write(&self, entry: WalEntry) -> Result<u64, WalError> {
        let mut inner = self.inner.lock().expect("WAL mutex poisoned");

        inner.sequence += 1;
        let seq = inner.sequence;

        let entry_json = serde_json::to_string(&entry)?;
        let crc = checksum(&entry_json);

        let record = WalRecord {
            seq,
            ts: unix_now(),
            crc,
            entry,
        };

        let mut line = serde_json::to_string(&record)?;
        line.push('\n');

        inner.file.write_all(line.as_bytes())?;
        inner.file.sync_all()?;

        Ok(seq)
    }

    /// Serialize the full application state as a `Checkpoint` entry and append
    /// it to the log.  Call this every second during an active sermon so that
    /// `replay()` never needs to walk more than one second of entries.
    pub fn checkpoint(&self, state: AppState) -> Result<u64, WalError> {
        self.write(WalEntry::Checkpoint { state })
    }

    /// Reconstruct the application state from the log.
    ///
    /// Algorithm:
    /// 1. Parse every line; verify its checksum.
    /// 2. Skip lines that are unparseable or have a wrong checksum (warning to
    ///    stderr).
    /// 3. Find the last valid `Checkpoint` entry — use it as the base state.
    /// 4. Apply every valid entry that follows the checkpoint in sequence.
    /// 5. Return the final state.
    ///
    /// Returns `AppState::default()` if the file does not exist or contains no
    /// usable entries.
    pub fn replay(&self) -> Result<AppState, WalError> {
        let file = match File::open(&self.path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(AppState::default());
            }
            Err(e) => return Err(e.into()),
        };

        let mut valid: Vec<(u64, WalEntry)> = Vec::new();

        for (line_num, line_result) in BufReader::new(file).lines().enumerate() {
            let line = match line_result {
                Ok(l) if l.trim().is_empty() => continue,
                Ok(l) => l,
                Err(e) => {
                    eprintln!("WAL replay: I/O error on line {}: {e}", line_num + 1);
                    continue;
                }
            };

            let record: WalRecord = match serde_json::from_str(&line) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!(
                        "WAL replay: skipping unparseable entry on line {} — {e}",
                        line_num + 1
                    );
                    continue;
                }
            };

            // Re-serialise the decoded entry and verify the checksum.
            let entry_json = serde_json::to_string(&record.entry)?;
            let expected = checksum(&entry_json);
            if record.crc != expected {
                eprintln!(
                    "WAL replay: checksum mismatch on seq {} (line {}), skipping",
                    record.seq,
                    line_num + 1
                );
                continue;
            }

            valid.push((record.seq, record.entry));
        }

        // Use the last valid checkpoint as the starting state.
        let (mut state, replay_from) = match valid
            .iter()
            .rposition(|(_, e)| matches!(e, WalEntry::Checkpoint { .. }))
        {
            Some(pos) => {
                let WalEntry::Checkpoint { state } = &valid[pos].1 else {
                    unreachable!()
                };
                (state.clone(), pos + 1)
            }
            None => (AppState::default(), 0),
        };

        // Apply every valid entry that came after the checkpoint.
        for (_, entry) in &valid[replay_from..] {
            apply_entry(&mut state, entry);
        }

        Ok(state)
    }
}

// ─── State reconstruction ─────────────────────────────────────────────────────

fn apply_entry(state: &mut AppState, entry: &WalEntry) {
    match entry {
        WalEntry::ChurchRegistered { church_id, name, region } => {
            state.church = Some(Church {
                id: church_id.clone(),
                name: name.clone(),
                region: region.clone(),
                installed_at: String::new(),
                onboarding_complete: false,
            });
        }
        WalEntry::SermonStarted { sermon_id, church_id, started_at } => {
            let date = started_at.get(..10).unwrap_or("").to_string();
            state.active_sermon = Some(Sermon {
                id: sermon_id.clone(),
                church_id: church_id.clone(),
                title: None,
                pastor: None,
                date,
                anchor_scripture: None,
                started_at: started_at.clone(),
                ended_at: None,
            });
            state.pending_detections.clear();
        }
        WalEntry::SermonEnded { sermon_id, ended_at } => {
            if let Some(ref mut sermon) = state.active_sermon {
                if sermon.id == *sermon_id {
                    sermon.ended_at = Some(ended_at.clone());
                }
            }
        }
        WalEntry::DetectionRecorded {
            event_id,
            sermon_id,
            raw_transcript,
            final_reference,
            confidence,
            decision,
            processing_time_ms,
        } => {
            if !state.pending_detections.iter().any(|d| d.id == *event_id) {
                state.pending_detections.push(DetectionEvent {
                    id: event_id.clone(),
                    sermon_id: sermon_id.clone(),
                    raw_transcript: raw_transcript.clone(),
                    pattern_result: None,
                    local_ai_result: None,
                    cloud_ai_result: None,
                    final_reference: final_reference.clone(),
                    confidence: *confidence,
                    decision: decision.clone(),
                    operator_action: None,
                    correct_reference: None,
                    processing_time_ms: *processing_time_ms,
                    timestamp: String::new(),
                });
            }
        }
        WalEntry::OperatorCorrected { event_id, action, correct_reference } => {
            if let Some(det) =
                state.pending_detections.iter_mut().find(|d| d.id == *event_id)
            {
                det.operator_action = Some(action.clone());
                det.correct_reference = correct_reference.clone();
            }
        }
        WalEntry::SettingChanged { key, value } => {
            state.settings.insert(key.clone(), value.clone());
        }
        WalEntry::CalibrationUpdated { church_id, stage, accept_above, escalate_below } => {
            if let Some(ct) = state.calibration.iter_mut().find(|c| c.stage == *stage) {
                ct.accept_above = *accept_above;
                ct.escalate_below = *escalate_below;
            } else {
                state.calibration.push(CalibrationThresholds {
                    id: String::new(),
                    church_id: church_id.clone(),
                    stage: stage.clone(),
                    accept_above: *accept_above,
                    escalate_below: *escalate_below,
                    updated_at: String::new(),
                });
            }
        }
        // ServiceRecordSaved is terminal — no in-memory state to update.
        // Checkpoint is a base, not a delta — never applied as an update.
        WalEntry::ServiceRecordSaved { .. } | WalEntry::Checkpoint { .. } => {}
    }
}

// ─── Private helpers ──────────────────────────────────────────────────────────

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

/// FNV-1a 64-bit hash — stable across Rust releases, no external dependencies.
fn checksum(data: &str) -> u64 {
    const FNV_OFFSET: u64 = 14695981039346656037;
    const FNV_PRIME: u64 = 1099511628211;
    data.bytes()
        .fold(FNV_OFFSET, |hash, byte| (hash ^ byte as u64).wrapping_mul(FNV_PRIME))
}
