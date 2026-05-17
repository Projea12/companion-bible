use companion_events::BibleReference;

use crate::{
    error::DisplayError,
    history::StateHistory,
    state::{DisplayedState, SubPoint},
    wal::{WalEntry, WriteAheadLog},
};

type Renderer = Box<dyn Fn(&DisplayedState) -> Result<(), String> + Send>;

pub struct DisplayController {
    current: DisplayedState,
    history: StateHistory,
    wal: Box<dyn WriteAheadLog>,
    renderer: Renderer,
}

impl DisplayController {
    pub fn new(wal: impl WriteAheadLog + 'static, renderer: Renderer) -> Self {
        Self {
            current: DisplayedState::Blank,
            history: StateHistory::new(),
            wal: Box::new(wal),
            renderer,
        }
    }

    // ── Observers ─────────────────────────────────────────────────────────────

    pub fn state(&self) -> &DisplayedState {
        &self.current
    }

    pub fn is_blank(&self) -> bool {
        matches!(self.current, DisplayedState::Blank)
    }

    pub fn history_len(&self) -> usize {
        self.history.len()
    }

    // ── Transitions ───────────────────────────────────────────────────────────

    pub fn show_blank(&mut self) -> Result<(), DisplayError> {
        self.transition(DisplayedState::Blank)
    }

    pub fn show_sermon_title(&mut self, title: impl Into<String>) -> Result<(), DisplayError> {
        self.transition(DisplayedState::SermonTitle(title.into()))
    }

    pub fn show_sub_point(&mut self, sub_point: SubPoint) -> Result<(), DisplayError> {
        self.transition(DisplayedState::SubPoint(sub_point))
    }

    pub fn show_verse(
        &mut self,
        reference: BibleReference,
        text: impl Into<String>,
    ) -> Result<(), DisplayError> {
        self.transition(DisplayedState::Verse(reference, text.into()))
    }

    /// Restore the previous state from history.
    pub fn discard(&mut self) -> Result<(), DisplayError> {
        let prev = self.history.pop().ok_or(DisplayError::NoHistory)?;

        let entry = WalEntry {
            from: self.current.clone(),
            to: prev.clone(),
        };
        self.wal
            .append(entry)
            .map_err(DisplayError::WalWriteFailed)?;

        self.current = prev;
        self.renderer(&self.current.clone())
    }

    // ── Private ───────────────────────────────────────────────────────────────

    fn transition(&mut self, next: DisplayedState) -> Result<(), DisplayError> {
        let entry = WalEntry {
            from: self.current.clone(),
            to: next.clone(),
        };
        self.wal
            .append(entry)
            .map_err(DisplayError::WalWriteFailed)?;

        self.history.push(self.current.clone());
        self.current = next;
        self.renderer(&self.current.clone())
    }

    fn renderer(&self, state: &DisplayedState) -> Result<(), DisplayError> {
        (self.renderer)(state).map_err(DisplayError::RenderFailed)
    }
}
