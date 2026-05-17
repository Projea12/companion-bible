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
pub struct FailingWal {
    pub message: String,
}

impl FailingWal {
    pub fn new(message: impl Into<String>) -> Self {
        Self { message: message.into() }
    }
}

impl WriteAheadLog for FailingWal {
    fn append(&mut self, _entry: WalEntry) -> Result<(), String> {
        Err(self.message.clone())
    }
}
