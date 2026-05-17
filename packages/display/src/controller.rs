use companion_events::BibleReference;

use crate::state::{DisplayedState, SubPoint};

/// Owns the current congregation display state and enforces valid transitions.
///
/// Callers mutate state through the typed methods rather than assigning
/// `DisplayedState` directly, which lets the controller enforce invariants
/// and keeps transition logic in one place.
pub struct DisplayController {
    current: DisplayedState,
}

impl Default for DisplayController {
    fn default() -> Self {
        Self::new()
    }
}

impl DisplayController {
    /// Create a new controller; the display starts blank.
    pub fn new() -> Self {
        Self {
            current: DisplayedState::Blank,
        }
    }

    // ── Observers ─────────────────────────────────────────────────────────────

    /// The current display state.
    pub fn state(&self) -> &DisplayedState {
        &self.current
    }

    /// `true` when the display is blank (no content shown).
    pub fn is_blank(&self) -> bool {
        matches!(self.current, DisplayedState::Blank)
    }

    // ── Transitions ───────────────────────────────────────────────────────────

    /// Black out the congregation display entirely.
    pub fn show_blank(&mut self) {
        self.current = DisplayedState::Blank;
    }

    /// Show a sermon title full-screen.
    pub fn show_sermon_title(&mut self, title: impl Into<String>) {
        self.current = DisplayedState::SermonTitle(title.into());
    }

    /// Show a sermon outline sub-point.
    pub fn show_sub_point(&mut self, sub_point: SubPoint) {
        self.current = DisplayedState::SubPoint(sub_point);
    }

    /// Show a scripture verse with its reference.
    pub fn show_verse(&mut self, reference: BibleReference, text: impl Into<String>) {
        self.current = DisplayedState::Verse(reference, text.into());
    }
}
