use std::sync::{Arc, Mutex};

use crate::state::DisplayedState;

#[derive(Debug, Clone, PartialEq)]
pub struct WalEntry {
    pub from: DisplayedState,
    pub to: DisplayedState,
}

pub trait WriteAheadLog: Send {
    fn append(&mut self, entry: WalEntry) -> Result<(), String>;
}

/// In-memory WAL — always succeeds; records entries for test inspection.
pub struct MemoryWal {
    pub entries: Vec<WalEntry>,
}

impl MemoryWal {
    pub fn new() -> Self {
        Self { entries: Vec::new() }
    }
}

impl WriteAheadLog for MemoryWal {
    fn append(&mut self, entry: WalEntry) -> Result<(), String> {
        self.entries.push(entry);
        Ok(())
    }
}

/// WAL that always fails — used to test error propagation.
#[allow(dead_code)]
pub struct FailingWal {
    pub message: String,
}

impl FailingWal {
    #[allow(dead_code)]
    pub fn new(message: impl Into<String>) -> Self {
        Self { message: message.into() }
    }
}

impl WriteAheadLog for FailingWal {
    fn append(&mut self, _entry: WalEntry) -> Result<(), String> {
        Err(self.message.clone())
    }
}

/// Shared WAL — test code retains a handle to inspect entries after the
/// controller takes ownership of the WAL itself.
pub struct SharedWal {
    entries: Arc<Mutex<Vec<WalEntry>>>,
}

impl SharedWal {
    /// Returns `(wal, shared_handle)`. Give `wal` to the controller; keep
    /// `shared_handle` in the test to read entries at any point.
    pub fn new() -> (Self, Arc<Mutex<Vec<WalEntry>>>) {
        let entries = Arc::new(Mutex::new(Vec::new()));
        (Self { entries: entries.clone() }, entries)
    }
}

impl WriteAheadLog for SharedWal {
    fn append(&mut self, entry: WalEntry) -> Result<(), String> {
        self.entries.lock().unwrap().push(entry);
        Ok(())
    }
}
