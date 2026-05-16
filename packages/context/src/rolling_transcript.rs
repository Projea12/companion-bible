//! Time-bounded sliding window of recent transcript text.

use std::collections::VecDeque;

/// Default window size — retains the last 60 seconds of speech.
pub const DEFAULT_WINDOW_MS: u64 = 60_000;

/// A time-bounded queue of recent transcript segments.
///
/// Each segment is stored with its audio timestamp.  When [`push`] is called,
/// segments whose timestamp falls more than [`window_ms`] before the incoming
/// segment are evicted.  [`text`] joins all retained segments with spaces.
///
/// The most recently pushed segment is always retained, even if it would
/// otherwise fall outside the window on its own.
///
/// [`push`]: RollingTranscript::push
/// [`window_ms`]: RollingTranscript::window_ms
/// [`text`]: RollingTranscript::text
pub struct RollingTranscript {
    /// `(corrected_text, audio_ms)` — most recent entry at the back.
    segments: VecDeque<(String, u64)>,
    current_chars: usize,
    window_ms: u64,
}

impl RollingTranscript {
    /// Create a window retaining the last [`DEFAULT_WINDOW_MS`] (60 s).
    pub fn new() -> Self {
        Self::with_window(DEFAULT_WINDOW_MS)
    }

    /// Create a window of a custom duration.
    pub fn with_window(window_ms: u64) -> Self {
        Self {
            segments: VecDeque::new(),
            current_chars: 0,
            window_ms,
        }
    }

    /// Append `text` spoken at `audio_ms` milliseconds from the stream start.
    ///
    /// Segments whose timestamp is strictly less than
    /// `audio_ms - window_ms` are evicted.  Empty strings are silently
    /// ignored.
    pub fn push(&mut self, text: &str, audio_ms: u64) {
        if text.is_empty() {
            return;
        }
        self.segments.push_back((text.to_owned(), audio_ms));
        self.current_chars += text.len();

        // Evict expired segments.  Always keep at least the segment we just
        // added (len > 1 guard).
        let cutoff = audio_ms.saturating_sub(self.window_ms);
        while self.segments.len() > 1 {
            match self.segments.front() {
                Some((_, ts)) if *ts < cutoff => {
                    let (evicted, _) = self.segments.pop_front().unwrap();
                    self.current_chars = self.current_chars.saturating_sub(evicted.len());
                }
                _ => break,
            }
        }
    }

    /// Return all retained segments joined by single spaces.
    pub fn text(&self) -> String {
        self.segments
            .iter()
            .map(|(t, _)| t.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// `true` when no segments are retained.
    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    /// Total character count across all retained segments.
    pub fn char_count(&self) -> usize {
        self.current_chars
    }

    /// The configured window size in milliseconds.
    pub fn window_ms(&self) -> u64 {
        self.window_ms
    }

    /// Clear all retained segments.
    pub fn clear(&mut self) {
        self.segments.clear();
        self.current_chars = 0;
    }
}

impl Default for RollingTranscript {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Construction ──────────────────────────────────────────────────────────

    #[test]
    fn new_is_empty_with_default_window() {
        let rt = RollingTranscript::new();
        assert!(rt.is_empty());
        assert_eq!(rt.char_count(), 0);
        assert_eq!(rt.text(), "");
        assert_eq!(rt.window_ms(), DEFAULT_WINDOW_MS);
    }

    #[test]
    fn with_window_sets_custom_duration() {
        let rt = RollingTranscript::with_window(30_000);
        assert_eq!(rt.window_ms(), 30_000);
        assert!(rt.is_empty());
    }

    // ── Basic push / retrieval ────────────────────────────────────────────────

    #[test]
    fn push_single_segment_retained() {
        let mut rt = RollingTranscript::new();
        rt.push("Romans 8:1", 0);
        assert_eq!(rt.text(), "Romans 8:1");
        assert!(!rt.is_empty());
    }

    #[test]
    fn push_multiple_segments_joined_with_space() {
        let mut rt = RollingTranscript::new();
        rt.push("Good morning.", 0);
        rt.push("Turn to John 3:16.", 1_000);
        assert_eq!(rt.text(), "Good morning. Turn to John 3:16.");
    }

    #[test]
    fn push_empty_string_is_ignored() {
        let mut rt = RollingTranscript::new();
        rt.push("", 0);
        assert!(rt.is_empty());
        rt.push("hello", 0);
        rt.push("", 1_000);
        assert_eq!(rt.text(), "hello");
    }

    #[test]
    fn char_count_tracks_retained_content() {
        let mut rt = RollingTranscript::new();
        rt.push("hello", 0);    // 5 chars
        assert_eq!(rt.char_count(), 5);
        rt.push("world", 1_000); // 5 chars
        assert_eq!(rt.char_count(), 10);
    }

    // ── 60-second retention ───────────────────────────────────────────────────

    #[test]
    fn holds_last_60_seconds_by_default() {
        let mut rt = RollingTranscript::new(); // 60 s window
        rt.push("early segment", 0);
        rt.push("recent segment", 61_000); // 61 s later → early evicted
        assert!(
            !rt.text().contains("early segment"),
            "segment older than 60 s should be evicted"
        );
        assert!(rt.text().contains("recent segment"));
    }

    #[test]
    fn retains_all_segments_within_60_second_window() {
        let mut rt = RollingTranscript::new();
        rt.push("first",  0);
        rt.push("second", 30_000); // 30 s
        rt.push("third",  59_000); // 59 s — all within 60 s of each other
        assert!(rt.text().contains("first"));
        assert!(rt.text().contains("second"));
        assert!(rt.text().contains("third"));
    }

    #[test]
    fn efficient_retrieval_across_many_segments() {
        let mut rt = RollingTranscript::new();
        // Fill the window with segments 1 second apart.
        for i in 0..60u64 {
            rt.push(&format!("segment_{i}"), i * 1_000);
        }
        // All 60 segments are within the 60-second window.
        let text = rt.text();
        assert!(text.contains("segment_0"));
        assert!(text.contains("segment_59"));
    }

    // ── Eviction boundary conditions ──────────────────────────────────────────

    #[test]
    fn evicts_segment_strictly_outside_window() {
        // cutoff = 60_001 - 60_000 = 1; segment at t=0 → 0 < 1 → evicted
        let mut rt = RollingTranscript::with_window(60_000);
        rt.push("old",    0);
        rt.push("recent", 60_001);
        assert!(!rt.text().contains("old"), "segment just outside window should be evicted");
        assert!(rt.text().contains("recent"));
    }

    #[test]
    fn retains_segment_exactly_at_window_boundary() {
        // cutoff = 60_000 - 60_000 = 0; segment at t=0 → 0 < 0 is false → kept
        let mut rt = RollingTranscript::with_window(60_000);
        rt.push("boundary", 0);
        rt.push("current",  60_000);
        assert!(rt.text().contains("boundary"), "segment at exact boundary should be retained");
        assert!(rt.text().contains("current"));
    }

    #[test]
    fn evicts_multiple_expired_segments_in_one_push() {
        // window = 10 s; push four segments, then jump to t=15 s
        let mut rt = RollingTranscript::with_window(10_000);
        rt.push("t0",  0);      // cutoff at t=15 s: 15000-10000=5000; 0 < 5000 → evict
        rt.push("t4",  4_000);  // 4000 < 5000 → evict
        rt.push("t5",  5_000);  // 5000 < 5000 is false → keep
        rt.push("t15", 15_000);
        assert!(!rt.text().contains("t0"),  "t0 should be evicted");
        assert!(!rt.text().contains("t4"),  "t4 should be evicted");
        assert!(rt.text().contains("t5"),  "t5 is at the cutoff boundary, should be kept");
        assert!(rt.text().contains("t15"), "t15 should be kept");
    }

    #[test]
    fn most_recent_segment_always_retained() {
        // Even when the entire window is effectively zero, keep the last segment.
        let mut rt = RollingTranscript::with_window(1_000); // 1 s window
        rt.push("only segment", 0);
        rt.push("newer", 5_000); // 5 s later — "only segment" outside, but "newer" must be kept
        assert!(rt.text().contains("newer"));
    }

    #[test]
    fn custom_5_second_window() {
        let mut rt = RollingTranscript::with_window(5_000);
        rt.push("early",   0);
        rt.push("current", 6_000); // cutoff = 1000; early(t=0) < 1000 → evict
        assert!(!rt.text().contains("early"));
        assert!(rt.text().contains("current"));
    }

    #[test]
    fn char_count_decremented_after_eviction() {
        let mut rt = RollingTranscript::with_window(5_000);
        rt.push("hello", 0);   // 5 chars
        assert_eq!(rt.char_count(), 5);
        rt.push("world", 6_000); // evicts "hello"; 5 chars remain
        assert_eq!(rt.char_count(), 5);
    }

    // ── Clear ─────────────────────────────────────────────────────────────────

    #[test]
    fn clear_empties_buffer_and_resets_char_count() {
        let mut rt = RollingTranscript::new();
        rt.push("some text", 0);
        rt.clear();
        assert!(rt.is_empty());
        assert_eq!(rt.char_count(), 0);
        assert_eq!(rt.text(), "");
    }

    #[test]
    fn push_after_clear_works_normally() {
        let mut rt = RollingTranscript::new();
        rt.push("first", 0);
        rt.clear();
        rt.push("second", 0);
        assert_eq!(rt.text(), "second");
    }
}
