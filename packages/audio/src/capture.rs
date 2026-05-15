use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::error::AudioError;
use crate::input::AudioInput;
use crate::ring_buffer::RingBuffer;

// ─── Public types ─────────────────────────────────────────────────────────────

/// Events emitted by the monitor thread on the channel returned by
/// [`AudioCapture::subscribe`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureEvent {
    /// The audio level dropped to near-zero for longer than
    /// `zero_ticks_threshold × monitor_interval`; the device may have been
    /// unplugged.
    AudioInputLost,
    /// A reconnection attempt succeeded and audio is flowing again.
    AudioInputRestored,
}

/// Tuning knobs for the monitor thread.  Pass to [`AudioCapture::with_config`].
pub struct CaptureConfig {
    /// Consecutive silent ticks before declaring a disconnect.
    /// Default: 3  (300 ms at the default 100 ms interval).
    pub zero_ticks_threshold: u32,
    /// How often the monitor wakes to update the level and check for silence.
    /// Default: 100 ms.
    pub monitor_interval: Duration,
    /// How long to wait between reconnection attempts after a disconnect.
    /// Default: 2 s.
    pub reconnect_interval: Duration,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            zero_ticks_threshold: 3,
            monitor_interval: Duration::from_millis(100),
            reconnect_interval: Duration::from_secs(2),
        }
    }
}

// ─── AudioCapture ─────────────────────────────────────────────────────────────

/// Dedicated audio capture manager.
///
/// ## Responsibilities
/// * Opens the selected device via the [`AudioInput`] implementation.
///   cpal schedules its callback on a platform-native real-time thread
///   (CoreAudio on macOS; on Linux the callback thread is upgraded to
///   `SCHED_FIFO` priority 80 on the first invocation).
/// * The cpal callback writes every audio chunk straight into the shared
///   [`RingBuffer`].  Nothing else happens on that hot path.
/// * A lightweight monitor thread wakes every `monitor_interval` (default
///   100 ms) to:
///   - Detect whether the callback has fired since the last tick (via a
///     sequence counter) and apply IIR smoothing to the peak level for the
///     operator UI meter; when the callback stops, the level decays to 0.
///   - Count consecutive silent ticks; after `zero_ticks_threshold` sends
///     [`CaptureEvent::AudioInputLost`] and enters reconnection mode.
///   - Retry the device every `reconnect_interval`; on success sends
///     [`CaptureEvent::AudioInputRestored`].
pub struct AudioCapture {
    input: Arc<Mutex<Box<dyn AudioInput>>>,
    buffer: Arc<RingBuffer<f32>>,
    /// f32::to_bits() of the current smoothed peak level; written only by the
    /// monitor thread so that `current_level()` always returns a stable value.
    level: Arc<AtomicU32>,
    /// Raw peak written by the cpal callback on every invocation.
    level_raw: Arc<AtomicU32>,
    /// Incremented by the cpal callback on every invocation.  The monitor
    /// compares its local `last_seq` to this to detect whether new audio
    /// arrived during the last tick window.
    callback_seq: Arc<AtomicUsize>,
    /// `true` while the stream is running and the device is responding.
    connected: Arc<AtomicBool>,
    stop_flag: Arc<AtomicBool>,
    config: CaptureConfig,
    monitor_handle: Option<JoinHandle<()>>,
    event_tx: mpsc::SyncSender<CaptureEvent>,
    event_rx: Option<mpsc::Receiver<CaptureEvent>>,
}

impl AudioCapture {
    /// Create a capture manager with default configuration.
    pub fn new(input: Box<dyn AudioInput>, buffer: Arc<RingBuffer<f32>>) -> Self {
        Self::with_config(input, buffer, CaptureConfig::default())
    }

    /// Create a capture manager with custom configuration.
    pub fn with_config(
        input: Box<dyn AudioInput>,
        buffer: Arc<RingBuffer<f32>>,
        config: CaptureConfig,
    ) -> Self {
        let (tx, rx) = mpsc::sync_channel(32);
        Self {
            input: Arc::new(Mutex::new(input)),
            buffer,
            level: Arc::new(AtomicU32::new(0)),
            level_raw: Arc::new(AtomicU32::new(0)),
            callback_seq: Arc::new(AtomicUsize::new(0)),
            connected: Arc::new(AtomicBool::new(false)),
            stop_flag: Arc::new(AtomicBool::new(true)),
            config,
            monitor_handle: None,
            event_tx: tx,
            event_rx: Some(rx),
        }
    }

    // ── Configuration ─────────────────────────────────────────────────────────

    /// Take the event receiver.  Can only be called once; returns `None` if
    /// already taken.
    pub fn subscribe(&mut self) -> Option<mpsc::Receiver<CaptureEvent>> {
        self.event_rx.take()
    }

    // ── Observability ─────────────────────────────────────────────────────────

    /// Smoothed peak level in [0.0, 1.0] for the operator UI level meter.
    ///
    /// Updated every `monitor_interval` with a first-order IIR (α = 0.3).
    /// Decays to 0 when the callback stops firing (device silent or unplugged).
    pub fn current_level(&self) -> f32 {
        f32::from_bits(self.level.load(Ordering::Relaxed))
    }

    /// `true` while the capture stream is running and the device is responding.
    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Acquire)
    }

    // ── Lifecycle ─────────────────────────────────────────────────────────────

    /// Open the device and start capturing.
    ///
    /// Safe to call on an already-running capture — the previous stream and
    /// monitor thread are stopped first.
    pub fn start(&mut self) -> Result<(), AudioError> {
        self.stop_join();

        self.stop_flag.store(false, Ordering::Release);
        self.connected.store(true, Ordering::Release);
        // Reset counters so the new stream starts fresh.
        self.callback_seq.store(0, Ordering::Release);
        self.level_raw.store(0, Ordering::Relaxed);
        self.level.store(0, Ordering::Relaxed);

        // ── cpal callback — the hot path ──────────────────────────────────────
        //
        // Runs on cpal's audio thread.  On the first invocation, tries to
        // upgrade that thread's scheduling policy to real-time.
        // Per-invocation work: compute peak, store raw level, increment seq,
        // write to ring buffer.  No allocation, no lock on the hot path.
        let buffer = Arc::clone(&self.buffer);
        let level_raw = Arc::clone(&self.level_raw);
        let callback_seq = Arc::clone(&self.callback_seq);
        let rt_done = Arc::new(AtomicBool::new(false));
        {
            let rt_done2 = Arc::clone(&rt_done);
            let mut inp = self.input.lock().unwrap();
            inp.start(Box::new(move |samples: Vec<f32>| {
                if !rt_done2.swap(true, Ordering::Relaxed) {
                    try_set_realtime_priority();
                }
                let peak = samples.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
                level_raw.store(peak.to_bits(), Ordering::Relaxed);
                callback_seq.fetch_add(1, Ordering::Release);
                buffer.write(&samples);
            }))?;
        }

        // ── Monitor thread — level smoothing + disconnect + reconnect ─────────
        let input = Arc::clone(&self.input);
        let buffer_mon = Arc::clone(&self.buffer);
        let level_mon = Arc::clone(&self.level);
        let level_raw_mon = Arc::clone(&self.level_raw);
        let callback_seq_mon = Arc::clone(&self.callback_seq);
        let connected_mon = Arc::clone(&self.connected);
        let stop_flag_mon = Arc::clone(&self.stop_flag);
        let event_tx = self.event_tx.clone();
        let cfg = MonitorCfg {
            zero_ticks_threshold: self.config.zero_ticks_threshold,
            monitor_interval: self.config.monitor_interval,
            reconnect_interval: self.config.reconnect_interval,
        };

        self.monitor_handle = Some(
            std::thread::Builder::new()
                .name("audio-capture-monitor".into())
                .spawn(move || {
                    monitor_loop(
                        input,
                        buffer_mon,
                        level_mon,
                        level_raw_mon,
                        callback_seq_mon,
                        connected_mon,
                        stop_flag_mon,
                        event_tx,
                        cfg,
                    );
                })
                .expect("failed to spawn audio-capture-monitor"),
        );

        Ok(())
    }

    /// Stop capturing and wait for the monitor thread to exit.
    pub fn stop(&mut self) {
        self.stop_join();
    }

    // ── Internal ──────────────────────────────────────────────────────────────

    fn stop_join(&mut self) {
        self.stop_flag.store(true, Ordering::Release);
        self.connected.store(false, Ordering::Release);
        if let Ok(mut inp) = self.input.lock() {
            inp.stop();
        }
        self.level.store(0u32, Ordering::Relaxed);
        self.level_raw.store(0u32, Ordering::Relaxed);
        if let Some(h) = self.monitor_handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for AudioCapture {
    fn drop(&mut self) {
        // Signal and drop without joining to avoid blocking the dropping thread.
        self.stop_flag.store(true, Ordering::Release);
        self.connected.store(false, Ordering::Release);
        if let Ok(mut inp) = self.input.lock() {
            inp.stop();
        }
        // Drop the handle; the monitor exits on its next stop_flag check.
        self.monitor_handle.take();
    }
}

// ─── Monitor loop ─────────────────────────────────────────────────────────────

struct MonitorCfg {
    zero_ticks_threshold: u32,
    monitor_interval: Duration,
    reconnect_interval: Duration,
}

#[allow(clippy::too_many_arguments)]
fn monitor_loop(
    input: Arc<Mutex<Box<dyn AudioInput>>>,
    buffer: Arc<RingBuffer<f32>>,
    level: Arc<AtomicU32>,
    level_raw: Arc<AtomicU32>,
    callback_seq: Arc<AtomicUsize>,
    connected: Arc<AtomicBool>,
    stop_flag: Arc<AtomicBool>,
    event_tx: mpsc::SyncSender<CaptureEvent>,
    cfg: MonitorCfg,
) {
    let mut zero_ticks: u32 = 0;
    let mut last_reconnect = Instant::now()
        .checked_sub(cfg.reconnect_interval)
        .unwrap_or_else(Instant::now);
    let mut smoothed: f32 = 0.0;

    // Grace period: let the stream settle before we start counting silence.
    sleep_interruptible(cfg.monitor_interval * 2, &stop_flag);

    // Baseline the sequence counter after the grace period so any callbacks
    // that fired during startup are treated as known.  The first_tick flag
    // gives one additional tick of margin: if the audio thread was scheduled
    // late (e.g. under test load) and hasn't fired yet by the end of grace,
    // the first post-grace tick is still counted as "flowing" rather than
    // triggering a false-positive disconnect.
    let mut last_seq = callback_seq.load(Ordering::Acquire);
    let mut first_tick = true;

    while !stop_flag.load(Ordering::Acquire) {
        let seq = callback_seq.load(Ordering::Acquire);
        let has_new_audio = seq != last_seq || first_tick;
        first_tick = false;
        last_seq = seq;

        if has_new_audio {
            // Callback fired at least once since the last tick — audio is flowing.
            let instant = f32::from_bits(level_raw.load(Ordering::Relaxed));
            smoothed += (instant - smoothed) * 0.3;
            zero_ticks = 0;
        } else {
            // No new audio since the last tick: decay the smoothed level toward 0
            // so the meter and disconnect detection reflect the silence correctly.
            smoothed *= 0.7;
            if connected.load(Ordering::Acquire) {
                zero_ticks += 1;
            }
        }

        // Publish smoothed level for current_level().
        level.store(smoothed.to_bits(), Ordering::Relaxed);

        if connected.load(Ordering::Acquire) {
            // ── Disconnect detection ───────────────────────────────────────────
            if zero_ticks >= cfg.zero_ticks_threshold {
                connected.store(false, Ordering::Release);
                let _ = event_tx.try_send(CaptureEvent::AudioInputLost);
                last_reconnect = Instant::now()
                    .checked_sub(cfg.reconnect_interval)
                    .unwrap_or_else(Instant::now);
                zero_ticks = 0;
            }
        } else {
            // ── Reconnection ───────────────────────────────────────────────────
            if last_reconnect.elapsed() >= cfg.reconnect_interval {
                last_reconnect = Instant::now();

                let buffer2 = Arc::clone(&buffer);
                let level_raw2 = Arc::clone(&level_raw);
                let callback_seq2 = Arc::clone(&callback_seq);
                let pre_seq = callback_seq.load(Ordering::Acquire);

                let result = {
                    let mut inp = input.lock().unwrap();
                    inp.start(Box::new(move |samples: Vec<f32>| {
                        let peak = samples.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
                        level_raw2.store(peak.to_bits(), Ordering::Relaxed);
                        callback_seq2.fetch_add(1, Ordering::Release);
                        buffer2.write(&samples);
                    }))
                };

                if result.is_ok() {
                    // Give the stream a moment to produce audio, then probe via
                    // the sequence counter (avoids reading a stale raw level).
                    sleep_interruptible(cfg.monitor_interval * 2, &stop_flag);
                    if stop_flag.load(Ordering::Acquire) {
                        return;
                    }
                    let post_seq = callback_seq.load(Ordering::Acquire);
                    if post_seq != pre_seq {
                        // Callback fired during the probe window — device is back.
                        let probe = f32::from_bits(level_raw.load(Ordering::Relaxed));
                        smoothed = probe;
                        last_seq = post_seq;
                        zero_ticks = 0;
                        connected.store(true, Ordering::Release);
                        let _ = event_tx.try_send(CaptureEvent::AudioInputRestored);
                    }
                }
            }
        }

        sleep_interruptible(cfg.monitor_interval, &stop_flag);
    }
}

/// Sleep for `duration`, waking early if `stop_flag` becomes true.
/// Checks the flag every 10 ms so the thread is responsive to stop requests.
fn sleep_interruptible(duration: Duration, stop_flag: &AtomicBool) {
    const TICK: Duration = Duration::from_millis(10);
    let deadline = Instant::now() + duration;
    while Instant::now() < deadline {
        if stop_flag.load(Ordering::Acquire) {
            return;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        std::thread::sleep(remaining.min(TICK));
    }
}

// ─── Real-time thread priority ────────────────────────────────────────────────

/// Best-effort: upgrade the calling thread to real-time scheduling priority.
///
/// Called on the first invocation of the cpal callback, which runs on cpal's
/// audio thread.
///
/// * **macOS**: CoreAudio already schedules the audio thread as a Mach
///   real-time thread; this function is a no-op.
/// * **Linux**: Requests `SCHED_FIFO` priority 80.  Succeeds only when the
///   process has `CAP_SYS_NICE` or the user's `RLIMIT_RTPRIO` allows it;
///   failure is silently ignored so the stream still works in environments
///   without special privileges.
fn try_set_realtime_priority() {
    #[cfg(target_os = "linux")]
    unsafe {
        let param = libc::sched_param { sched_priority: 80 };
        // Return value intentionally ignored — failure is non-fatal.
        let _ = libc::sched_setscheduler(0, libc::SCHED_FIFO, &param);
    }
}
