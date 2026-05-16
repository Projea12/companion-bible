//! Fixed-capacity sliding window of recent transcript text.

use std::collections::VecDeque;

/// Default maximum character count retained across all segments.
///
/// At ~150 words-per-minute and ~5 chars/word, 2 000 chars ≈ 16 seconds of
/// speech — enough to resolve a typical scripture preamble.
pub const DEFAULT_ROLLING_CAPACITY: usize = 2_000;

/// A bounded queue of recent transcript text segments.
///
/// Segments are appended with [`push`][RollingTranscript::push]; the oldest
/// are evicted when the total character count would exceed `max_chars`.
/// [`text`][RollingTranscript::text] joins all retained segments with spaces.
pub struct RollingTranscript {
    segments: VecDeque<String>,
    current_chars: usize,
    max_chars: usize,
}

impl RollingTranscript {
    /// Create a window with the default capacity ([`DEFAULT_ROLLING_CAPACITY`]).
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_ROLLING_CAPACITY)
    }

    /// Create a window that retains at most `max_chars` characters.
    pub fn with_capacity(max_chars: usize) -> Self {
        Self {
            segments: VecDeque::new(),
            current_chars: 0,
            max_chars: max_chars.max(1),
        }
    }

    /// Append `text` to the window, evicting the oldest segment(s) if the
    /// capacity would be exceeded.
    ///
    /// Empty strings are silently ignored.  A single segment that exceeds the
    /// capacity on its own is still retained (the window always holds at least
    /// the most recent segment).
    pub fn push(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.current_chars += text.len();
        self.segments.push_back(text.to_string());

        // Evict oldest segments until under budget, but always keep at least
        // the segment we just added.
        while self.current_chars > self.max_chars && self.segments.len() > 1 {
            if let Some(evicted) = self.segments.pop_front() {
                self.current_chars = self.current_chars.saturating_sub(evicted.len());
            }
        }
    }

    /// Return all retained segments joined by single spaces.
    pub fn text(&self) -> String {
        self.segments
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// `true` when no segments are retained.
    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    /// Total character count currently held across all retained segments.
    pub fn char_count(&self) -> usize {
        self.current_chars
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

    #[test]
    fn new_is_empty() {
        let rt = RollingTranscript::new();
        assert!(rt.is_empty());
        assert_eq!(rt.char_count(), 0);
        assert_eq!(rt.text(), "");
    }

    #[test]
    fn push_single_segment() {
        let mut rt = RollingTranscript::new();
        rt.push("Romans 8:1");
        assert_eq!(rt.text(), "Romans 8:1");
        assert!(!rt.is_empty());
    }

    #[test]
    fn push_multiple_segments_joined_with_space() {
        let mut rt = RollingTranscript::new();
        rt.push("Good morning.");
        rt.push("Turn to John 3:16.");
        assert_eq!(rt.text(), "Good morning. Turn to John 3:16.");
    }

    #[test]
    fn push_empty_string_is_ignored() {
        let mut rt = RollingTranscript::new();
        rt.push("");
        assert!(rt.is_empty());
        rt.push("hello");
        rt.push("");
        assert_eq!(rt.text(), "hello");
    }

    #[test]
    fn evicts_oldest_when_over_capacity() {
        // capacity = 20 chars; push three segments
        let mut rt = RollingTranscript::with_capacity(20);
        rt.push("abcdefghij"); // 10 chars
        rt.push("klmnopqrst"); // 10 chars — now at capacity
        rt.push("uvwxyz"); // 6 chars — must evict first
        assert!(
            !rt.text().contains("abcdefghij"),
            "oldest segment should be evicted"
        );
        assert!(rt.text().contains("klmnopqrst"));
        assert!(rt.text().contains("uvwxyz"));
    }

    #[test]
    fn single_oversized_segment_is_retained() {
        // Even if one segment exceeds capacity it must be kept.
        let mut rt = RollingTranscript::with_capacity(5);
        rt.push("this is much longer than five characters");
        assert!(!rt.is_empty());
        assert_eq!(rt.text(), "this is much longer than five characters");
    }

    #[test]
    fn exact_capacity_does_not_evict() {
        let mut rt = RollingTranscript::with_capacity(10);
        rt.push("hello"); // 5
        rt.push("world"); // 5 — exactly 10, no eviction
        assert_eq!(rt.char_count(), 10);
        assert!(rt.text().contains("hello"));
        assert!(rt.text().contains("world"));
    }

    #[test]
    fn clear_empties_buffer() {
        let mut rt = RollingTranscript::new();
        rt.push("some text");
        rt.clear();
        assert!(rt.is_empty());
        assert_eq!(rt.char_count(), 0);
        assert_eq!(rt.text(), "");
    }

    #[test]
    fn char_count_tracks_content() {
        let mut rt = RollingTranscript::new();
        rt.push("hello"); // 5
        assert_eq!(rt.char_count(), 5);
        rt.push("world"); // 5
        assert_eq!(rt.char_count(), 10);
    }

    #[test]
    fn push_after_clear_works_normally() {
        let mut rt = RollingTranscript::new();
        rt.push("first");
        rt.clear();
        rt.push("second");
        assert_eq!(rt.text(), "second");
    }

    #[test]
    fn evicts_multiple_segments_to_make_room() {
        // capacity = 15; push four 5-char segments — oldest two should be gone
        let mut rt = RollingTranscript::with_capacity(15);
        rt.push("aaaaa"); // 5
        rt.push("bbbbb"); // 5
        rt.push("ccccc"); // 5 — at capacity
        rt.push("ddddd"); // 5 — must evict "aaaaa"; total = 15, ok
        // Now add one more that forces "bbbbb" out too
        rt.push("eeeee"); // 5 — at 20 > 15: evict "bbbbb"; total = 15
        assert!(!rt.text().contains("aaaaa"));
        assert!(!rt.text().contains("bbbbb"));
        assert!(rt.text().contains("ccccc"));
        assert!(rt.text().contains("ddddd"));
        assert!(rt.text().contains("eeeee"));
    }
}
