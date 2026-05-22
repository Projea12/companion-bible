//! Tracks the active hymn playback state for one session.
//!
//! Once a hymn number is detected the `HymnSession` holds the full playback
//! sequence and the current position.  Each call to `check_advance` tests the
//! incoming transcription against the last line of the current section; when
//! it matches the session advances automatically.

use companion_hymns::{last_line_matches, Hymn, HymnBook};

// ─── HymnSessionEvent ─────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum HymnSessionEvent {
    /// A new hymn was loaded — show its first section immediately.
    Loaded {
        number: u16,
        title: String,
        section_index: usize,
        /// 1-based stanza number (None for chorus).
        stanza_number: Option<u16>,
        is_chorus: bool,
        lines: Vec<String>,
    },
    /// Advanced to the next section (auto or manual).
    Advanced {
        number: u16,
        section_index: usize,
        /// 1-based stanza number (None for chorus).
        stanza_number: Option<u16>,
        is_chorus: bool,
        lines: Vec<String>,
    },
    /// All sections displayed — hymn is complete.
    Completed { number: u16 },
}

// ─── HymnSession ──────────────────────────────────────────────────────────────

pub struct HymnSession {
    number: u16,
    title: String,
    /// Owned copy of the playback sequence lines.
    sequence: Vec<(bool, Vec<String>)>,
    /// Current position in the sequence (0-based).
    pub position: usize,
    /// True once the last section has been displayed.
    pub completed: bool,
}

impl HymnSession {
    /// Load a hymn by number.  Returns `None` if the number is not in the book.
    pub fn load(number: u16) -> Option<Self> {
        let book = HymnBook::global();
        let hymn: &Hymn = book.get(number)?;
        let sequence: Vec<(bool, Vec<String>)> = hymn
            .playback_sequence()
            .into_iter()
            .map(|s| (s.is_chorus(), s.lines().to_vec()))
            .collect();

        if sequence.is_empty() {
            return None;
        }

        Some(Self {
            number,
            title: hymn.title.clone(),
            sequence,
            position: 0,
            completed: false,
        })
    }

    /// The section currently on screen.
    pub fn current(&self) -> Option<&(bool, Vec<String>)> {
        self.sequence.get(self.position)
    }

    /// Count of non-chorus sections up to and including `pos` (1-based stanza number).
    /// Returns `None` if the section at `pos` is a chorus.
    fn stanza_number_at(&self, pos: usize) -> Option<u16> {
        let (is_chorus, _) = self.sequence.get(pos)?;
        if *is_chorus {
            return None;
        }
        let count = self.sequence[..=pos]
            .iter()
            .filter(|(chorus, _)| !chorus)
            .count() as u16;
        Some(count)
    }

    /// Emit the initial `Loaded` event for the first section.
    pub fn start_event(&self) -> Option<HymnSessionEvent> {
        let (is_chorus, lines) = self.current()?;
        Some(HymnSessionEvent::Loaded {
            number: self.number,
            title: self.title.clone(),
            section_index: self.position,
            stanza_number: self.stanza_number_at(self.position),
            is_chorus: *is_chorus,
            lines: lines.clone(),
        })
    }

    /// Check whether `text` matches the last line of the current section.
    /// If it does, advance and return the appropriate event.
    pub fn check_advance(&mut self, text: &str) -> Option<HymnSessionEvent> {
        if self.completed {
            return None;
        }
        let (_, lines) = self.current()?;
        let last = lines.iter().rev().find(|l| !l.trim().is_empty())?.as_str();

        if !last_line_matches(last, text) {
            return None;
        }

        self.advance()
    }

    /// Manually advance to the next section (operator button).
    pub fn advance(&mut self) -> Option<HymnSessionEvent> {
        if self.completed {
            return None;
        }

        self.position += 1;

        if self.position >= self.sequence.len() {
            self.completed = true;
            return Some(HymnSessionEvent::Completed {
                number: self.number,
            });
        }

        let (is_chorus, lines) = &self.sequence[self.position];
        Some(HymnSessionEvent::Advanced {
            number: self.number,
            section_index: self.position,
            stanza_number: self.stanza_number_at(self.position),
            is_chorus: *is_chorus,
            lines: lines.clone(),
        })
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_valid_hymn() {
        let s = HymnSession::load(1).unwrap();
        assert_eq!(s.number, 1);
        assert!(!s.completed);
        assert_eq!(s.position, 0);
    }

    #[test]
    fn load_invalid_returns_none() {
        assert!(HymnSession::load(999).is_none());
    }

    #[test]
    fn start_event_is_loaded() {
        let s = HymnSession::load(1).unwrap();
        assert!(matches!(
            s.start_event(),
            Some(HymnSessionEvent::Loaded { .. })
        ));
    }

    #[test]
    fn manual_advance_cycles_through_sequence() {
        let mut s = HymnSession::load(1).unwrap();
        let total = s.sequence.len();
        for i in 1..total {
            let ev = s.advance().unwrap();
            assert!(
                matches!(ev, HymnSessionEvent::Advanced { section_index, .. } if section_index == i)
            );
        }
        // Final advance completes the hymn.
        let ev = s.advance().unwrap();
        assert!(matches!(ev, HymnSessionEvent::Completed { .. }));
        assert!(s.completed);
    }

    #[test]
    fn check_advance_on_matching_last_line() {
        let mut s = HymnSession::load(1).unwrap();
        // Hymn 1 stanza 1 last line: "All your anxiety--leave it there."
        let ev = s.check_advance("all your anxiety leave it there");
        assert!(ev.is_some());
        assert_eq!(s.position, 1);
    }

    #[test]
    fn check_advance_no_match_does_not_advance() {
        let mut s = HymnSession::load(1).unwrap();
        s.check_advance("something completely unrelated");
        assert_eq!(s.position, 0);
    }
}
