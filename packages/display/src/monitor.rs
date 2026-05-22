/// A snapshot of the physical monitor layout at a single point in time.
/// Constructed by the Tauri layer and fed into `DisplayMonitor::update`.
#[derive(Debug, Clone, PartialEq)]
pub struct MonitorLayout {
    /// Total number of physical monitors currently detected.
    pub total: usize,
    /// `true` when the congregation window's position sits outside the
    /// primary monitor bounds — i.e. it is on the secondary screen.
    pub congregation_on_secondary: bool,
}

impl MonitorLayout {
    pub fn new(total: usize, congregation_on_secondary: bool) -> Self {
        Self {
            total,
            congregation_on_secondary,
        }
    }
}

/// Classification of the current screen configuration.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ScreenStatus {
    /// Secondary monitor present; congregation display correctly assigned.
    Connected,
    /// No secondary monitor — congregation display cannot be shown.
    Disconnected,
    /// Two or more monitors present but congregation window is on the
    /// primary screen. Operator and congregation appear to be swapped.
    Swapped,
}

impl std::fmt::Display for ScreenStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Connected => write!(f, "connected"),
            Self::Disconnected => write!(f, "disconnected"),
            Self::Swapped => write!(f, "swapped"),
        }
    }
}

/// Tracks screen connectivity, detects assignment problems, and reports
/// status changes.
///
/// Feed a fresh `MonitorLayout` into `update()` on every poll cycle.
/// The method returns `Some(new_status)` only when the status actually
/// changes, so callers can emit events without duplicating notifications.
pub struct DisplayMonitor {
    status: ScreenStatus,
}

impl DisplayMonitor {
    /// Create with a known initial layout (avoids a spurious event on the
    /// first poll cycle).
    pub fn new(initial: MonitorLayout) -> Self {
        Self {
            status: Self::resolve(&initial),
        }
    }

    /// The most recently resolved status.
    pub fn status(&self) -> &ScreenStatus {
        &self.status
    }

    /// Feed in the current monitor layout.
    ///
    /// Returns the new `ScreenStatus` if it changed, or `None` if unchanged.
    pub fn update(&mut self, layout: MonitorLayout) -> Option<ScreenStatus> {
        let next = Self::resolve(&layout);
        if next == self.status {
            return None;
        }
        self.status = next.clone();
        Some(next)
    }

    fn resolve(layout: &MonitorLayout) -> ScreenStatus {
        match layout.total {
            0 | 1 => ScreenStatus::Disconnected,
            _ if layout.congregation_on_secondary => ScreenStatus::Connected,
            _ => ScreenStatus::Swapped,
        }
    }
}
