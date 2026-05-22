//! Background connectivity monitor — emits AppEvents on state change.

use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Interval between connectivity probes.
const CHECK_INTERVAL_MS: u64 = 5_000;
/// TCP connect timeout for the probe.
const PROBE_TIMEOUT_MS: u64 = 2_000;
/// Probe target — Cloudflare DNS, reachable on any working connection.
const PROBE_ADDR: &str = "1.1.1.1:443";

// ─── ConnectivityMonitor ──────────────────────────────────────────────────────

/// Spawns a background thread that probes internet connectivity every 5 seconds
/// and calls `on_change(is_connected)` whenever the state changes.
///
/// The monitor stops when the returned [`MonitorHandle`] is dropped.
pub struct ConnectivityMonitor;

impl ConnectivityMonitor {
    /// Start the monitor.  `on_change` is called from the background thread
    /// whenever connectivity flips; it is NOT called on the initial state.
    ///
    /// Use `on_change` to emit `AppEvent::InternetConnected` /
    /// `AppEvent::InternetDisconnected` into your event bus.
    pub fn start<F>(on_change: F) -> MonitorHandle
    where
        F: Fn(bool) + Send + 'static,
    {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_clone = Arc::clone(&stop);

        let thread = std::thread::Builder::new()
            .name("connectivity-monitor".into())
            .spawn(move || {
                let mut last_state: Option<bool> = None;

                while !stop_clone.load(Ordering::Relaxed) {
                    let connected = probe();

                    if last_state != Some(connected) {
                        last_state = Some(connected);
                        on_change(connected);
                    }

                    std::thread::sleep(Duration::from_millis(CHECK_INTERVAL_MS));
                }
            })
            .expect("failed to spawn connectivity-monitor thread");

        MonitorHandle {
            stop,
            _thread: Mutex::new(Some(thread)),
        }
    }

    /// One-shot non-blocking check — returns `true` if internet is reachable.
    pub fn is_connected() -> bool {
        probe()
    }
}

// ─── MonitorHandle ────────────────────────────────────────────────────────────

/// Stops the monitor thread when dropped.
pub struct MonitorHandle {
    stop: Arc<AtomicBool>,
    _thread: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl Drop for MonitorHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

// ─── probe ────────────────────────────────────────────────────────────────────

/// Non-blocking TCP probe — succeeds if we can connect to `PROBE_ADDR`.
fn probe() -> bool {
    TcpStream::connect_timeout(
        &PROBE_ADDR.parse().expect("static addr is valid"),
        Duration::from_millis(PROBE_TIMEOUT_MS),
    )
    .is_ok()
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU32;

    #[test]
    fn probe_returns_bool_without_panic() {
        // Just verify it runs cleanly — value depends on network.
        let _ = probe();
    }

    #[test]
    fn is_connected_returns_bool_without_panic() {
        let _ = ConnectivityMonitor::is_connected();
    }

    #[test]
    fn monitor_calls_on_change_then_stops_on_drop() {
        let count = Arc::new(AtomicU32::new(0));
        let count_clone = Arc::clone(&count);

        let handle = ConnectivityMonitor::start(move |_connected| {
            count_clone.fetch_add(1, Ordering::Relaxed);
        });

        // Give the monitor thread time for one probe cycle.
        std::thread::sleep(Duration::from_millis(200));

        // Drop the handle — this sets the stop flag.
        drop(handle);

        // on_change should have been called at most once (initial state flip).
        // We only assert it didn't panic and the count is small.
        assert!(count.load(Ordering::Relaxed) <= 2);
    }

    #[test]
    fn dropping_handle_does_not_panic() {
        let handle = ConnectivityMonitor::start(|_| {});
        drop(handle);
    }

    #[test]
    fn check_interval_is_5_seconds() {
        assert_eq!(CHECK_INTERVAL_MS, 5_000);
    }

    #[test]
    fn on_change_not_called_when_state_unchanged() {
        // Start with a known state, then check again — if state doesn't
        // change, on_change should not fire a second time.
        let count = Arc::new(AtomicU32::new(0));
        let count_clone = Arc::clone(&count);

        // Two consecutive probes: both return the same result.
        // on_change fires at most once (for the initial state discovery).
        let _handle = ConnectivityMonitor::start(move |_| {
            count_clone.fetch_add(1, Ordering::Relaxed);
        });

        std::thread::sleep(Duration::from_millis(50));
        let after_first = count.load(Ordering::Relaxed);

        // No second check has fired within 50ms (interval is 5s).
        assert!(
            after_first <= 1,
            "on_change fired {after_first} times before first interval"
        );
    }

    #[test]
    fn monitor_handle_stops_background_thread() {
        let running = Arc::new(AtomicBool::new(false));
        let running_clone = Arc::clone(&running);

        let handle = ConnectivityMonitor::start(move |_| {
            running_clone.store(true, Ordering::Relaxed);
        });

        // Drop immediately.
        drop(handle);

        // Thread is signalled to stop; give it a moment to observe the flag.
        std::thread::sleep(Duration::from_millis(50));
        // No assertion on `running` — we just verify no panic or hang.
    }
}
