use std::collections::VecDeque;

use crate::state::DisplayedState;

const CAPACITY: usize = 10;

pub struct StateHistory {
    entries: VecDeque<DisplayedState>,
}

impl StateHistory {
    pub fn new() -> Self {
        Self {
            entries: VecDeque::with_capacity(CAPACITY),
        }
    }

    pub fn push(&mut self, state: DisplayedState) {
        if self.entries.len() == CAPACITY {
            self.entries.pop_front();
        }
        self.entries.push_back(state);
    }

    /// Remove and return the most recent entry.
    pub fn pop(&mut self) -> Option<DisplayedState> {
        self.entries.pop_back()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
