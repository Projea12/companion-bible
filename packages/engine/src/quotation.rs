//! Quotation-matching layer.
//!
//! Detects Bible verses being *read aloud* by comparing the rolling transcript
//! against KJV verse text via FTS5. Unlike the pattern layer (which catches
//! explicit citations like "John 3:16"), this layer catches the preacher
//! actually reading verse text — e.g. "for God so loved the world that he gave
//! his only begotten son" — without naming the reference.
//!
//! ## How it works
//!
//! 1. FTS5 returns the top candidates ranked by BM25 (most transcript words
//!    found in the verse).
//! 2. Word-overlap scoring filters candidates: we compute what fraction of the
//!    verse's content words appear in the transcript.
//! 3. Only candidates above [`MIN_OVERLAP`] are accepted.

use companion_arbitrator::LayerResult;
use companion_database::FtsResult;

/// Minimum fraction of verse content words that must appear in the transcript.
const MIN_OVERLAP: f32 = 0.50;

/// Minimum verse word count. Short verses (e.g. "Jesus wept.") produce too
/// many false positives — skip them.
const MIN_VERSE_WORDS: usize = 6;

/// KJV stop words excluded from overlap scoring.
const STOP_WORDS: &[&str] = &[
    "a",
    "an",
    "the",
    "and",
    "or",
    "but",
    "in",
    "on",
    "at",
    "to",
    "for",
    "of",
    "with",
    "by",
    "from",
    "is",
    "was",
    "are",
    "were",
    "be",
    "been",
    "have",
    "has",
    "had",
    "do",
    "does",
    "did",
    "will",
    "would",
    "shall",
    "should",
    "may",
    "might",
    "must",
    "can",
    "could",
    "not",
    "no",
    "it",
    "its",
    "he",
    "she",
    "they",
    "we",
    "you",
    "i",
    "me",
    "him",
    "her",
    "them",
    "us",
    "his",
    "her",
    "their",
    "our",
    "your",
    "my",
    "who",
    "which",
    "that",
    "this",
    "these",
    "those",
    "there",
    "so",
    "thy",
    "thee",
    "thou",
    "thine",
    "ye",
    "hath",
    "hast",
    "doth",
    "shalt",
    "unto",
    "upon",
    "thereof",
    "wherein",
    "therefore",
    "wherefore",
];

// ─── Public API ───────────────────────────────────────────────────────────────

/// Given FTS5 candidates and the rolling transcript, return the best-matching
/// verse as a `LayerResult`, or `None` if no candidate meets the threshold.
pub fn best_quotation_match(candidates: &[FtsResult], transcript: &str) -> Option<LayerResult> {
    let transcript_words = content_word_set(transcript);
    if transcript_words.is_empty() {
        return None;
    }

    let mut best: Option<(LayerResult, f32)> = None;

    for candidate in candidates {
        let verse_words = content_words(&candidate.text);
        if verse_words.len() < MIN_VERSE_WORDS {
            continue;
        }

        let matched = verse_words
            .iter()
            .filter(|w| transcript_words.contains(w.as_str()))
            .count();
        let overlap = matched as f32 / verse_words.len() as f32;

        if overlap < MIN_OVERLAP {
            continue;
        }

        let confidence = overlap_to_confidence(overlap);
        let chapter = candidate.chapter.try_into().ok()?;
        let verse_num = candidate.verse_number.try_into().ok()?;

        let result = LayerResult {
            book: Some(candidate.book.clone()),
            chapter: Some(chapter),
            verse: Some(verse_num),
            confidence,
        };

        if best.as_ref().map(|(_, c)| overlap > *c).unwrap_or(true) {
            best = Some((result, overlap));
        }
    }

    best.map(|(r, _)| r)
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn overlap_to_confidence(overlap: f32) -> f32 {
    if overlap >= 0.80 {
        0.93
    } else if overlap >= 0.65 {
        0.85
    } else {
        0.72
    }
}

/// Return content words from `text` as a lowercase Vec (with duplicates).
fn content_words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphabetic())
        .filter(|w| w.len() >= 3 && !STOP_WORDS.contains(w))
        .map(String::from)
        .collect()
}

/// Return content words from `text` as a lowercase HashSet (for fast lookup).
fn content_word_set(text: &str) -> std::collections::HashSet<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphabetic())
        .filter(|w| w.len() >= 3 && !STOP_WORDS.contains(w))
        .map(String::from)
        .collect()
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_candidate(book: &str, chapter: i64, verse: i64, text: &str) -> FtsResult {
        FtsResult {
            book: book.to_string(),
            chapter,
            verse_number: verse,
            text: text.to_string(),
            rank: -1.0,
        }
    }

    #[test]
    fn detects_john_3_16_by_text() {
        let candidates = vec![make_candidate(
            "John",
            3,
            16,
            "For God so loved the world that he gave his only begotten Son \
             that whosoever believeth in him should not perish but have everlasting life",
        )];
        let transcript = "for god so loved the world that he gave his only begotten son \
             that whosoever believeth in him should not perish";
        let result = best_quotation_match(&candidates, transcript).unwrap();
        assert_eq!(result.book.as_deref(), Some("John"));
        assert_eq!(result.chapter, Some(3));
        assert_eq!(result.verse, Some(16));
        assert!(result.confidence >= 0.85);
    }

    #[test]
    fn rejects_low_overlap() {
        let candidates = vec![make_candidate(
            "Romans",
            8,
            28,
            "And we know that all things work together for good to them that love God \
             to them who are the called according to his purpose",
        )];
        // Only 2 words match — well below 50% threshold
        let transcript = "we know";
        assert!(best_quotation_match(&candidates, transcript).is_none());
    }

    #[test]
    fn skips_short_verses() {
        // "Jesus wept." — only 2 words, below MIN_VERSE_WORDS
        let candidates = vec![make_candidate("John", 11, 35, "Jesus wept")];
        let transcript = "Jesus wept in the garden";
        assert!(best_quotation_match(&candidates, transcript).is_none());
    }

    #[test]
    fn picks_best_when_multiple_candidates() {
        let candidates = vec![
            make_candidate("Psalms", 23, 1, "The LORD is my shepherd I shall not want"),
            make_candidate(
                "John",
                3,
                16,
                "For God so loved the world that he gave his only begotten Son \
                 that whosoever believeth in him should not perish but have everlasting life",
            ),
        ];
        // Transcript clearly matches John 3:16
        let transcript =
            "god loved world gave only begotten son whosoever believeth perish everlasting";
        let result = best_quotation_match(&candidates, transcript).unwrap();
        assert_eq!(result.book.as_deref(), Some("John"));
        assert_eq!(result.chapter, Some(3));
    }
}
