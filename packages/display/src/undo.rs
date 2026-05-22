use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crate::state::DisplayedState;

pub type ActionId = u64;

/// How long an undo action remains available after it is recorded.
pub const UNDO_WINDOW: Duration = Duration::from_secs(5);

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum UndoError {
    #[error("action {0} not found")]
    NotFound(ActionId),
    /// The action existed but the 5-second window has closed.
    #[error("action {0} has expired")]
    Expired(ActionId),
}

/// A single recoverable discard recorded in the undo stack.
pub struct UndoableAction {
    pub id: ActionId,
    pub previous_state: DisplayedState,
    recorded_at: Instant,
}

/// Time-bounded undo stack.
///
/// Every time-sensitive method accepts `now: Instant` so callers can
/// inject a known timestamp — no sleeps or mocking needed in tests.
pub struct UndoSystem {
    stack: VecDeque<UndoableAction>,
    next_id: ActionId,
}

impl Default for UndoSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl UndoSystem {
    pub fn new() -> Self {
        Self {
            stack: VecDeque::new(),
            next_id: 1,
        }
    }

    // ── recording ──────────────────────────────────────────────────────────────

    /// Push a recoverable discard onto the stack.
    ///
    /// Returns the `ActionId` which the Tauri layer should forward to the
    /// operator UI so it can display the correct undo button.
    pub fn record_discard(&mut self, previous_state: DisplayedState, now: Instant) -> ActionId {
        let id = self.next_id;
        self.next_id += 1;
        self.stack.push_back(UndoableAction {
            id,
            previous_state,
            recorded_at: now,
        });
        id
    }

    // ── undo ───────────────────────────────────────────────────────────────────

    /// Attempt to undo the action identified by `action_id`.
    ///
    /// On success, removes the action from the stack and returns the
    /// `DisplayedState` to restore.  The caller is responsible for actually
    /// restoring the state (via `DisplayController`) and writing to the WAL.
    pub fn undo(&mut self, action_id: ActionId, now: Instant) -> Result<DisplayedState, UndoError> {
        let pos = self.stack.iter().position(|a| a.id == action_id);
        let Some(pos) = pos else {
            return Err(UndoError::NotFound(action_id));
        };
        if now.duration_since(self.stack[pos].recorded_at) >= UNDO_WINDOW {
            self.stack.remove(pos);
            return Err(UndoError::Expired(action_id));
        }
        let action = self.stack.remove(pos).unwrap();
        Ok(action.previous_state)
    }

    // ── expiry ─────────────────────────────────────────────────────────────────

    /// Purge all actions whose undo window has closed.
    ///
    /// Returns the IDs of expired actions so the operator UI can hide
    /// their corresponding undo buttons.
    pub fn expire_old(&mut self, now: Instant) -> Vec<ActionId> {
        let mut expired = Vec::new();
        self.stack.retain(|a| {
            if now.duration_since(a.recorded_at) >= UNDO_WINDOW {
                expired.push(a.id);
                false
            } else {
                true
            }
        });
        expired
    }

    // ── observers ──────────────────────────────────────────────────────────────

    pub fn len(&self) -> usize {
        self.stack.len()
    }

    pub fn is_empty(&self) -> bool {
        self.stack.is_empty()
    }

    /// `true` if the action exists and its undo window has not yet closed.
    pub fn is_within_window(&self, action_id: ActionId, now: Instant) -> bool {
        self.stack
            .iter()
            .find(|a| a.id == action_id)
            .map(|a| now.duration_since(a.recorded_at) < UNDO_WINDOW)
            .unwrap_or(false)
    }
}
