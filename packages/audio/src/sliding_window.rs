use std::collections::VecDeque;
use std::time::{Duration, Instant};

// ─── Constants ────────────────────────────────────────────────────────────────

/// Default sample rate assumed by `SlidingWindow::new()`.
pub const SAMPLE_RATE: u32 = 16_000;

/// Maximum buffered audio in seconds.
pub const WINDOW_SECS: u32 = 30;

/// Maximum samples in a default-capacity window (16 000 Hz × 30 s).
pub const WINDOW_CAPACITY: usize = SAMPLE_RATE as usize * WINDOW_SECS as usize;

// ─── AudioWindow ──────────────────────────────────────────────────────────────

/// A contiguous slice of audio returned by [`SlidingWindow::last`].
#[derive(Debug, Clone)]
pub struct AudioWindow {
    /// Mono f32 samples in [-1, 1].
    pub samples: Vec<f32>,
    /// Sample rate of the captured audio.
    pub sample_rate: u32,
}

impl AudioWindow {
    /// Wall-clock duration represented by this window.
    pub fn duration(&self) -> Duration {
        if self.sample_rate == 0 {
            return Duration::ZERO;
        }
        Duration::from_secs_f64(self.samples.len() as f64 / self.sample_rate as f64)
    }

    /// `true` when the window contains no samples.
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }
}

// ─── SlidingWindow ────────────────────────────────────────────────────────────

/// Rolling buffer of clean audio, sized for `WINDOW_SECS` (30 s) of audio.
///
/// ## Usage pattern
/// The preprocessing pipeline feeds cleaned 100 ms chunks via [`push`].
/// The transcription scheduler polls [`new_audio_since`] to decide whether
/// there is fresh audio worth sending to Whisper, then extracts the relevant
/// span with [`last`].
///
/// ## Drop-oldest semantics
/// When the buffer is full, the oldest samples are silently discarded to make
/// room for the incoming chunk.  Audio that has already been transcribed
/// should be trimmed by the caller via [`trim_front`] so the buffer does not
/// fill up with already-processed data.
///
/// ## Thread safety
/// `SlidingWindow` is **not** thread-safe.  Wrap in `Arc<Mutex<_>>` or
/// confine to a single thread.
pub struct SlidingWindow {
    buf: VecDeque<f32>,
    capacity: usize,
    sample_rate: u32,
    last_push: Option<Instant>,
}

impl SlidingWindow {
    /// Create a window sized for [`WINDOW_SECS`] seconds at [`SAMPLE_RATE`] Hz.
    pub fn new() -> Self {
        Self::with_params(SAMPLE_RATE, WINDOW_SECS)
    }

    /// Create a window with an explicit sample rate and duration.
    pub fn with_params(sample_rate: u32, max_secs: u32) -> Self {
        let capacity = sample_rate as usize * max_secs as usize;
        Self {
            buf: VecDeque::with_capacity(capacity),
            capacity,
            sample_rate,
            last_push: None,
        }
    }

    // ── Observability ─────────────────────────────────────────────────────────

    /// Number of samples currently buffered.
    pub fn len(&self) -> usize {
        self.buf.len()
    }

    /// `true` when no samples have been pushed yet.
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    /// Wall-clock duration of audio currently in the buffer.
    pub fn duration_buffered(&self) -> Duration {
        if self.sample_rate == 0 {
            return Duration::ZERO;
        }
        Duration::from_secs_f64(self.buf.len() as f64 / self.sample_rate as f64)
    }

    /// Maximum duration the window can hold.
    pub fn max_duration(&self) -> Duration {
        Duration::from_secs_f64(self.capacity as f64 / self.sample_rate as f64)
    }

    /// `Instant` of the most recent [`push`] call, or `None` if nothing has
    /// been pushed yet.
    pub fn last_push_time(&self) -> Option<Instant> {
        self.last_push
    }

    // ── Core API ──────────────────────────────────────────────────────────────

    /// Append `chunk` to the buffer, dropping the oldest samples if necessary.
    ///
    /// If `chunk` is longer than the window capacity only the newest
    /// `capacity` samples are kept.
    pub fn push(&mut self, chunk: &[f32]) {
        if chunk.is_empty() {
            return;
        }

        // If the chunk itself exceeds the window capacity, keep only the tail.
        let chunk = if chunk.len() > self.capacity {
            &chunk[chunk.len() - self.capacity..]
        } else {
            chunk
        };

        // Drop oldest samples to make room.
        let used = self.buf.len();
        if used + chunk.len() > self.capacity {
            let excess = (used + chunk.len()) - self.capacity;
            self.buf.drain(..excess);
        }

        self.buf.extend(chunk.iter().copied());
        self.last_push = Some(Instant::now());
    }

    /// Return the last `duration` of buffered audio.
    ///
    /// If fewer samples than requested are available, all buffered samples are
    /// returned.  The result is always a contiguous `Vec<f32>` — callers may
    /// pass it directly to Whisper without further copying.
    pub fn last(&self, duration: Duration) -> AudioWindow {
        let n_requested =
            (duration.as_secs_f64() * self.sample_rate as f64).round() as usize;
        let n = n_requested.min(self.buf.len());
        let start = self.buf.len() - n;
        let samples: Vec<f32> = self.buf.range(start..).copied().collect();
        AudioWindow { samples, sample_rate: self.sample_rate }
    }

    /// Return `true` if audio has been pushed **after** `timestamp`.
    ///
    /// Used by the transcription scheduler to decide whether to trigger a
    /// Whisper inference run:
    ///
    /// ```text
    /// let checkpoint = Instant::now();
    /// // … wait for VAD silence …
    /// if window.new_audio_since(checkpoint) {
    ///     let audio = window.last(Duration::from_secs(10));
    ///     transcribe(audio);
    /// }
    /// ```
    pub fn new_audio_since(&self, timestamp: Instant) -> bool {
        self.last_push.map(|t| t > timestamp).unwrap_or(false)
    }

    /// Remove the oldest `duration` of buffered audio.
    ///
    /// Call this after a chunk has been transcribed to reclaim capacity.
    /// If `duration` exceeds the buffered audio, the buffer is cleared.
    pub fn trim_front(&mut self, duration: Duration) {
        let n = (duration.as_secs_f64() * self.sample_rate as f64).round() as usize;
        let n = n.min(self.buf.len());
        self.buf.drain(..n);
    }

    /// Drain all buffered samples into a `Vec<f32>`.
    pub fn drain_all(&mut self) -> Vec<f32> {
        self.buf.drain(..).collect()
    }
}

impl Default for SlidingWindow {
    fn default() -> Self {
        Self::new()
    }
}
