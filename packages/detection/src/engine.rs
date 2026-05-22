//! Pattern-matching engine for spoken scripture references (Layer 1).
//!
//! The engine receives text that has already been processed by:
//! - [`correction::correct_text`][crate] — Nigerian-English phonetic fixes
//! - [`NumberNormalizer`][crate::NumberNormalizer] — word numbers → digits
//!
//! It then applies a priority-ordered set of regexes to find every scripture
//! reference and return structured [`PatternResult`] values.
//!
//! ## Pattern priority
//!
//! | Priority | Pattern                              | Confidence |
//! |---------|--------------------------------------|------------|
//! | 1       | `Book N:N` (colon form)              | 1.00       |
//! | 2       | `Book [chapter] N verse N`           | 0.95       |
//! | 2b      | `Book N N` (space-separated)         | 0.90       |
//! | 3       | `Book [chapter] N` only              | 0.70       |
//! | 4       | `chapter N verse N` (no book)        | 0.60       |
//! | 5       | `verse N` only                       | 0.40       |
//!
//! Matches are deduplicated by byte range: a higher-priority match consumes
//! its span so that lower-priority patterns cannot also fire there.
//!
//! ## False-positive prevention
//!
//! - Book names are only matched when followed by a digit or "chapter"/"verse"
//!   keyword, so "John went to the store" does not match.
//! - A negative lookahead after the chapter digit rejects phone-number
//!   patterns like "John 333-4444".
//! - Only numbers 1–199 are accepted as chapter/verse values (the range that
//!   covers every book in the Bible).

use regex::Regex;

use crate::book_data::{build_book_alternation, canonical_name};

// ─── Public types ─────────────────────────────────────────────────────────────

/// How complete the matched reference is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchCompleteness {
    /// `Book N:N` — fully specified, colon separator (e.g. "John 3:16").
    FullCanonical,
    /// `Book [chapter] N verse N` — fully specified, spoken keywords.
    BookChapterVerse,
    /// `Book N N` — fully specified, space-separated (e.g. "Jude 1 5").
    BookChapterVerseSpaced,
    /// `Book [chapter] N` — book and chapter, no verse.
    BookChapter,
    /// `chapter N verse N` — no book name.
    ChapterVerse,
    /// `verse N` — verse number only.
    VerseOnly,
}

/// A single scripture-reference candidate found by the pattern engine.
#[derive(Debug, Clone, PartialEq)]
pub struct PatternResult {
    /// Canonical book name (e.g. `"1 Corinthians"`), or `None` when only a
    /// chapter/verse was matched with no book context.
    pub book: Option<String>,

    /// Chapter number, or `None` for verse-only matches.
    pub chapter: Option<u8>,

    /// Verse number, or `None` for book+chapter-only matches.
    pub verse: Option<u8>,

    /// Confidence that this is a genuine scripture reference [0.0 – 1.0].
    pub confidence: f32,

    /// What parts of the reference were present.
    pub completeness: MatchCompleteness,

    /// Byte offset of the first character of the match in the input string.
    pub start: usize,

    /// Byte offset one past the last character of the match.
    pub end: usize,
}

impl PatternResult {
    fn confidence_for(c: MatchCompleteness) -> f32 {
        match c {
            MatchCompleteness::FullCanonical => 1.00,
            MatchCompleteness::BookChapterVerse => 0.95,
            MatchCompleteness::BookChapterVerseSpaced => 0.90,
            MatchCompleteness::BookChapter => 0.70,
            MatchCompleteness::ChapterVerse => 0.60,
            MatchCompleteness::VerseOnly => 0.40,
        }
    }
}

// ─── PatternEngine ────────────────────────────────────────────────────────────

/// Compiled pattern engine.  Create one instance and reuse it — construction
/// compiles all regexes and is expensive relative to matching.
pub struct PatternEngine {
    /// `Book N:N` — highest confidence.
    colon_re: Regex,
    /// `Book [chapter] N verse N` — spoken form.
    spoken_re: Regex,
    /// `Book N N` — space-separated chapter and verse.
    space_re: Regex,
    /// `Book [chapter] N` only.
    book_chapter_re: Regex,
    /// `chapter N verse N` — no book.
    chapter_verse_re: Regex,
    /// `verse N` — verse only.
    verse_only_re: Regex,
}

// Optional spoken preambles that may precede a reference.
//
// "Turn to Romans 8", "Open your Bibles to John 3:16",
// "The book of Genesis chapter 1 verse 1".
const PREAMBLE: &str = r"(?:(?:the\s+book\s+of|turn(?:ing)?\s+(?:your\s+bibles?\s+)?to|open(?:ing)?\s+(?:your\s+bibles?\s+)?(?:to|at))\s+)?";

impl PatternEngine {
    /// Compile all patterns.  This allocates; call once and cache the result.
    pub fn new() -> Self {
        let book_alt = build_book_alternation();

        // ── Pattern 1: "Book N:N" ─────────────────────────────────────────────
        // Separator: colon, slash, or "and" (Nigerian English: "Genesis 1 and 1").
        let colon_re = Regex::new(&(
            "(?i)".to_owned()
                + PREAMBLE
                + r"(?P<book>"
                + &book_alt
                + r")\s+(?:chapter\s+)?(?P<chapter>[1-9]\d{0,2})(?:\s*[:/]\s*|\s+and\s+)(?P<verse>[1-9]\d{0,2})"
        )).expect("colon_re");

        // ── Pattern 2: "Book [chapter] N verse N" ────────────────────────────
        // Allow an optional comma before "verse" — AssemblyAI smart punctuation
        // emits "John chapter 3, verse 16" (colon form is handled by pattern 1).
        let spoken_re = Regex::new(&(
            "(?i)".to_owned()
                + PREAMBLE
                + r"(?P<book>"
                + &book_alt
                + r")\s+(?:chapter\s+)?(?P<chapter>[1-9]\d{0,2})\s*,?\s*verse\s+(?P<verse>[1-9]\d{0,2})\b"
        )).expect("spoken_re");

        // ── Pattern 2b: "Book N N" — space-separated chapter + verse ─────────
        // Handles spoken references like "Jude 1 5" (after NumberNormalizer).
        // Lower confidence than colon/spoken forms (0.90) since it's ambiguous.
        // The \s+ between numbers must not cross a "verse" keyword — that case
        // is already consumed by spoken_re above via deduplication.
        let space_re = Regex::new(
            &("(?i)".to_owned()
                + PREAMBLE
                + r"(?P<book>"
                + &book_alt
                + r")\s+(?:chapter\s+)?(?P<chapter>[1-9]\d{0,2})\s+(?P<verse>[1-9]\d{0,2})\b"),
        )
        .expect("space_re");

        // ── Pattern 3: "Book [chapter] N" only ───────────────────────────────
        // Colon-form and spoken-form overlaps are handled by deduplication.
        // Phone-number guard is applied in collect_book_chapter via post-filter.
        let book_chapter_re = Regex::new(
            &("(?i)".to_owned()
                + PREAMBLE
                + r"(?P<book>"
                + &book_alt
                + r")\s+(?:chapter\s+)?(?P<chapter>[1-9]\d{0,2})\b"),
        )
        .expect("book_chapter_re");

        // ── Pattern 4: "chapter N verse N" (no book) ─────────────────────────
        // Allow optional comma — AssemblyAI emits "chapter 3, verse 16".
        let chapter_verse_re = Regex::new(
            r"(?i)\bchapter\s+(?P<chapter>[1-9]\d{0,2})\s*,?\s*verse\s+(?P<verse>[1-9]\d{0,2})\b",
        )
        .expect("chapter_verse_re");

        // ── Pattern 5: "verse N" only ─────────────────────────────────────────
        let verse_only_re =
            Regex::new(r"(?i)\bverse\s+(?P<verse>[1-9]\d{0,2})\b").expect("verse_only_re");

        Self {
            colon_re,
            spoken_re,
            space_re,
            book_chapter_re,
            chapter_verse_re,
            verse_only_re,
        }
    }

    // ── Public API ────────────────────────────────────────────────────────────

    /// Find every non-overlapping scripture reference in `text`.
    ///
    /// Results are returned sorted by start position.  When two patterns would
    /// match the same span, the higher-confidence pattern wins.
    ///
    /// ```rust
    /// use companion_detection::PatternEngine;
    ///
    /// let engine = PatternEngine::new();
    /// let results = engine.find_all("Turn to John 3:16.");
    /// assert_eq!(results.len(), 1);
    /// assert_eq!(results[0].book.as_deref(), Some("John"));
    /// assert_eq!(results[0].chapter, Some(3));
    /// assert_eq!(results[0].verse,   Some(16));
    /// assert_eq!(results[0].confidence, 1.0);
    /// ```
    pub fn find_all(&self, text: &str) -> Vec<PatternResult> {
        let mut candidates: Vec<PatternResult> = Vec::new();

        // Collect in descending confidence order so highest wins on overlap.
        self.collect_colon(text, &mut candidates);
        self.collect_spoken(text, &mut candidates);
        self.collect_space(text, &mut candidates);
        self.collect_book_chapter(text, &mut candidates);
        self.collect_chapter_verse(text, &mut candidates);
        self.collect_verse_only(text, &mut candidates);

        // Deduplicate: keep each match only if it does not overlap any
        // already-kept higher-priority match.
        let mut kept: Vec<PatternResult> = Vec::new();
        for c in candidates {
            if !kept
                .iter()
                .any(|k| overlaps(k.start..k.end, c.start..c.end))
            {
                kept.push(c);
            }
        }

        // Return sorted by position in the source text.
        kept.sort_by_key(|r| r.start);
        kept
    }

    // ── Collectors ───────────────────────────────────────────────────────────

    fn collect_colon(&self, text: &str, out: &mut Vec<PatternResult>) {
        for caps in self.colon_re.captures_iter(text) {
            let m = caps.get(0).unwrap();
            out.push(PatternResult {
                book: book_from_caps(&caps, "book"),
                chapter: num_from_caps(&caps, "chapter"),
                verse: num_from_caps(&caps, "verse"),
                confidence: PatternResult::confidence_for(MatchCompleteness::FullCanonical),
                completeness: MatchCompleteness::FullCanonical,
                start: m.start(),
                end: m.end(),
            });
        }
    }

    fn collect_spoken(&self, text: &str, out: &mut Vec<PatternResult>) {
        for caps in self.spoken_re.captures_iter(text) {
            let m = caps.get(0).unwrap();
            out.push(PatternResult {
                book: book_from_caps(&caps, "book"),
                chapter: num_from_caps(&caps, "chapter"),
                verse: num_from_caps(&caps, "verse"),
                confidence: PatternResult::confidence_for(MatchCompleteness::BookChapterVerse),
                completeness: MatchCompleteness::BookChapterVerse,
                start: m.start(),
                end: m.end(),
            });
        }
    }

    fn collect_space(&self, text: &str, out: &mut Vec<PatternResult>) {
        for caps in self.space_re.captures_iter(text) {
            let m = caps.get(0).unwrap();
            out.push(PatternResult {
                book: book_from_caps(&caps, "book"),
                chapter: num_from_caps(&caps, "chapter"),
                verse: num_from_caps(&caps, "verse"),
                confidence: PatternResult::confidence_for(
                    MatchCompleteness::BookChapterVerseSpaced,
                ),
                completeness: MatchCompleteness::BookChapterVerseSpaced,
                start: m.start(),
                end: m.end(),
            });
        }
    }

    fn collect_book_chapter(&self, text: &str, out: &mut Vec<PatternResult>) {
        for caps in self.book_chapter_re.captures_iter(text) {
            let m = caps.get(0).unwrap();
            // Phone-number guard: skip "Book NNN-NNNN" patterns.
            let after = &text[m.end()..];
            let after_trimmed = after.trim_start_matches([' ', '\t']);
            if let Some(rest) = after_trimmed.strip_prefix('-') {
                if rest.starts_with(|c: char| c.is_ascii_digit()) {
                    continue;
                }
            }
            out.push(PatternResult {
                book: book_from_caps(&caps, "book"),
                chapter: num_from_caps(&caps, "chapter"),
                verse: None,
                confidence: PatternResult::confidence_for(MatchCompleteness::BookChapter),
                completeness: MatchCompleteness::BookChapter,
                start: m.start(),
                end: m.end(),
            });
        }
    }

    fn collect_chapter_verse(&self, text: &str, out: &mut Vec<PatternResult>) {
        for caps in self.chapter_verse_re.captures_iter(text) {
            let m = caps.get(0).unwrap();
            out.push(PatternResult {
                book: None,
                chapter: num_from_caps(&caps, "chapter"),
                verse: num_from_caps(&caps, "verse"),
                confidence: PatternResult::confidence_for(MatchCompleteness::ChapterVerse),
                completeness: MatchCompleteness::ChapterVerse,
                start: m.start(),
                end: m.end(),
            });
        }
    }

    fn collect_verse_only(&self, text: &str, out: &mut Vec<PatternResult>) {
        for caps in self.verse_only_re.captures_iter(text) {
            let m = caps.get(0).unwrap();
            out.push(PatternResult {
                book: None,
                chapter: None,
                verse: num_from_caps(&caps, "verse"),
                confidence: PatternResult::confidence_for(MatchCompleteness::VerseOnly),
                completeness: MatchCompleteness::VerseOnly,
                start: m.start(),
                end: m.end(),
            });
        }
    }
}

impl Default for PatternEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn book_from_caps(caps: &regex::Captures, name: &str) -> Option<String> {
    caps.name(name)
        .map(|m| canonical_name(m.as_str()).unwrap_or(m.as_str()).to_string())
}

fn num_from_caps(caps: &regex::Captures, name: &str) -> Option<u8> {
    caps.name(name).and_then(|m| m.as_str().parse::<u8>().ok())
}

fn overlaps(a: std::ops::Range<usize>, b: std::ops::Range<usize>) -> bool {
    a.start < b.end && b.start < a.end
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn engine() -> PatternEngine {
        PatternEngine::new()
    }

    // ── Helper ────────────────────────────────────────────────────────────────

    fn first(text: &str) -> PatternResult {
        engine()
            .find_all(text)
            .into_iter()
            .next()
            .expect("expected a match")
    }

    fn none(text: &str) {
        let r = engine().find_all(text);
        assert!(r.is_empty(), "expected no match, got: {r:?} for '{text}'");
    }

    // ── Pattern 1: canonical colon form ──────────────────────────────────────

    #[test]
    fn pattern_canonical_john_3_16() {
        let r = first("John 3:16");
        assert_eq!(r.book.as_deref(), Some("John"));
        assert_eq!(r.chapter, Some(3));
        assert_eq!(r.verse, Some(16));
        assert_eq!(r.completeness, MatchCompleteness::FullCanonical);
        assert_eq!(r.confidence, 1.0);
    }

    #[test]
    fn pattern_canonical_genesis_1_1() {
        let r = first("Genesis 1:1");
        assert_eq!(r.book.as_deref(), Some("Genesis"));
        assert_eq!(r.chapter, Some(1));
        assert_eq!(r.verse, Some(1));
        assert_eq!(r.completeness, MatchCompleteness::FullCanonical);
    }

    #[test]
    fn pattern_canonical_revelation_22_21() {
        let r = first("Revelation 22:21");
        assert_eq!(r.book.as_deref(), Some("Revelation"));
        assert_eq!(r.chapter, Some(22));
        assert_eq!(r.verse, Some(21));
    }

    #[test]
    fn pattern_canonical_1_corinthians() {
        let r = first("1 Corinthians 13:13");
        assert_eq!(r.book.as_deref(), Some("1 Corinthians"));
        assert_eq!(r.chapter, Some(13));
        assert_eq!(r.verse, Some(13));
        assert_eq!(r.completeness, MatchCompleteness::FullCanonical);
    }

    #[test]
    fn pattern_canonical_psalm_23_1() {
        let r = first("Psalms 23:1");
        assert_eq!(r.book.as_deref(), Some("Psalms"));
        assert_eq!(r.chapter, Some(23));
        assert_eq!(r.verse, Some(1));
    }

    #[test]
    fn pattern_canonical_spacing_variants() {
        // The regex allows flexible whitespace around the colon.
        let r = first("Romans 8 : 1");
        assert_eq!(r.book.as_deref(), Some("Romans"));
        assert_eq!(r.chapter, Some(8));
        assert_eq!(r.verse, Some(1));
    }

    // ── Pattern 1 variant: "and" as verse separator ───────────────────────────

    #[test]
    fn pattern_and_separator_genesis_1_and_1() {
        let r = first("Genesis 1 and 1");
        assert_eq!(r.book.as_deref(), Some("Genesis"));
        assert_eq!(r.chapter, Some(1));
        assert_eq!(r.verse, Some(1));
        assert_eq!(r.completeness, MatchCompleteness::FullCanonical);
    }

    #[test]
    fn pattern_and_separator_john_3_and_16() {
        let r = first("John 3 and 16");
        assert_eq!(r.book.as_deref(), Some("John"));
        assert_eq!(r.chapter, Some(3));
        assert_eq!(r.verse, Some(16));
        assert_eq!(r.confidence, 1.0);
    }

    #[test]
    fn pattern_and_separator_romans_8_and_28() {
        let r = first("Romans chapter 8 and 28");
        assert_eq!(r.book.as_deref(), Some("Romans"));
        assert_eq!(r.chapter, Some(8));
        assert_eq!(r.verse, Some(28));
    }

    // ── Pattern 2: spoken form ────────────────────────────────────────────────

    #[test]
    fn pattern_spoken_john_chapter_3_verse_16() {
        let r = first("John chapter 3 verse 16");
        assert_eq!(r.book.as_deref(), Some("John"));
        assert_eq!(r.chapter, Some(3));
        assert_eq!(r.verse, Some(16));
        assert_eq!(r.completeness, MatchCompleteness::BookChapterVerse);
        assert_eq!(r.confidence, 0.95);
    }

    #[test]
    fn pattern_spoken_without_chapter_keyword() {
        // "John 3 verse 16" — "chapter" keyword omitted.
        let r = first("John 3 verse 16");
        assert_eq!(r.book.as_deref(), Some("John"));
        assert_eq!(r.chapter, Some(3));
        assert_eq!(r.verse, Some(16));
        assert_eq!(r.completeness, MatchCompleteness::BookChapterVerse);
    }

    #[test]
    fn pattern_spoken_romans_8_1() {
        let r = first("Romans chapter 8 verse 1");
        assert_eq!(r.book.as_deref(), Some("Romans"));
        assert_eq!(r.chapter, Some(8));
        assert_eq!(r.verse, Some(1));
    }

    #[test]
    fn pattern_spoken_1_thessalonians() {
        let r = first("1 Thessalonians chapter 4 verse 16");
        assert_eq!(r.book.as_deref(), Some("1 Thessalonians"));
        assert_eq!(r.chapter, Some(4));
        assert_eq!(r.verse, Some(16));
    }

    // ── Pattern 2 variant: preamble forms ────────────────────────────────────

    #[test]
    fn pattern_preamble_the_book_of() {
        let r = first("the book of John chapter 3 verse 16");
        assert_eq!(r.book.as_deref(), Some("John"));
        assert_eq!(r.chapter, Some(3));
        assert_eq!(r.verse, Some(16));
    }

    #[test]
    fn pattern_preamble_turn_to() {
        let r = first("turn to Romans 8:1");
        assert_eq!(r.book.as_deref(), Some("Romans"));
        assert_eq!(r.chapter, Some(8));
        assert_eq!(r.verse, Some(1));
        assert_eq!(r.completeness, MatchCompleteness::FullCanonical);
    }

    #[test]
    fn pattern_preamble_turn_to_spoken() {
        let r = first("Turn to John chapter 3 verse 16");
        assert_eq!(r.book.as_deref(), Some("John"));
        assert_eq!(r.completeness, MatchCompleteness::BookChapterVerse);
    }

    #[test]
    fn pattern_preamble_open_your_bibles() {
        let r = first("open your Bibles to Ephesians chapter 1 verse 3");
        assert_eq!(r.book.as_deref(), Some("Ephesians"));
        assert_eq!(r.chapter, Some(1));
        assert_eq!(r.verse, Some(3));
    }

    // ── Pattern 2b: space-separated book chapter verse ────────────────────────

    #[test]
    fn pattern_space_jude_1_5() {
        let r = first("Jude 1 5");
        assert_eq!(r.book.as_deref(), Some("Jude"));
        assert_eq!(r.chapter, Some(1));
        assert_eq!(r.verse, Some(5));
        assert_eq!(r.completeness, MatchCompleteness::BookChapterVerseSpaced);
        assert_eq!(r.confidence, 0.90);
    }

    #[test]
    fn pattern_space_john_3_16() {
        let r = first("John 3 16");
        assert_eq!(r.book.as_deref(), Some("John"));
        assert_eq!(r.chapter, Some(3));
        assert_eq!(r.verse, Some(16));
        assert_eq!(r.completeness, MatchCompleteness::BookChapterVerseSpaced);
    }

    #[test]
    fn pattern_space_beats_book_chapter() {
        // "Jude 1 5" should produce BookChapterVerseSpaced, not BookChapter.
        let results = engine().find_all("Jude 1 5");
        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].completeness,
            MatchCompleteness::BookChapterVerseSpaced
        );
    }

    #[test]
    fn pattern_colon_beats_space() {
        // "John 3:16" must still be FullCanonical, not BookChapterVerseSpaced.
        let results = engine().find_all("John 3:16");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].completeness, MatchCompleteness::FullCanonical);
    }

    #[test]
    fn pattern_spoken_beats_space() {
        // "John 3 verse 16" — spoken_re wins over space_re.
        let results = engine().find_all("John 3 verse 16");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].completeness, MatchCompleteness::BookChapterVerse);
    }

    #[test]
    fn confidence_book_chapter_verse_spaced_is_0_90() {
        assert_eq!(first("Jude 1 5").confidence, 0.90);
    }

    // ── Pattern 3: book + chapter only ───────────────────────────────────────

    #[test]
    fn pattern_book_chapter_only() {
        let r = first("Romans 8");
        assert_eq!(r.book.as_deref(), Some("Romans"));
        assert_eq!(r.chapter, Some(8));
        assert_eq!(r.verse, None);
        assert_eq!(r.completeness, MatchCompleteness::BookChapter);
        assert_eq!(r.confidence, 0.70);
    }

    #[test]
    fn pattern_turn_to_book_chapter() {
        let r = first("turn to Romans 8");
        assert_eq!(r.book.as_deref(), Some("Romans"));
        assert_eq!(r.chapter, Some(8));
        assert_eq!(r.verse, None);
        assert_eq!(r.completeness, MatchCompleteness::BookChapter);
    }

    #[test]
    fn pattern_book_chapter_with_keyword() {
        let r = first("John chapter 1");
        assert_eq!(r.book.as_deref(), Some("John"));
        assert_eq!(r.chapter, Some(1));
        assert_eq!(r.verse, None);
        assert_eq!(r.completeness, MatchCompleteness::BookChapter);
    }

    // ── Pattern 4: chapter + verse, no book ──────────────────────────────────

    #[test]
    fn pattern_chapter_verse_no_book() {
        let r = first("chapter 3 verse 16");
        assert_eq!(r.book, None);
        assert_eq!(r.chapter, Some(3));
        assert_eq!(r.verse, Some(16));
        assert_eq!(r.completeness, MatchCompleteness::ChapterVerse);
        assert_eq!(r.confidence, 0.60);
    }

    #[test]
    fn pattern_chapter_verse_in_sentence() {
        let r = first("We are looking at chapter 8 verse 1 today");
        assert_eq!(r.chapter, Some(8));
        assert_eq!(r.verse, Some(1));
        assert_eq!(r.completeness, MatchCompleteness::ChapterVerse);
    }

    // ── Pattern 5: verse only ─────────────────────────────────────────────────

    #[test]
    fn pattern_verse_only() {
        let r = first("verse 16");
        assert_eq!(r.book, None);
        assert_eq!(r.chapter, None);
        assert_eq!(r.verse, Some(16));
        assert_eq!(r.completeness, MatchCompleteness::VerseOnly);
        assert_eq!(r.confidence, 0.40);
    }

    #[test]
    fn pattern_verse_only_in_sentence() {
        let r = first("Let us read verse 13");
        assert_eq!(r.verse, Some(13));
        assert_eq!(r.completeness, MatchCompleteness::VerseOnly);
    }

    // ── Alias forms (abbreviations) ───────────────────────────────────────────

    #[test]
    fn alias_rom_abbreviation() {
        let r = first("Rom 8:1");
        assert_eq!(r.book.as_deref(), Some("Romans"));
    }

    #[test]
    fn alias_gen_abbreviation() {
        let r = first("Gen 1:1");
        assert_eq!(r.book.as_deref(), Some("Genesis"));
    }

    #[test]
    fn alias_ps_abbreviation() {
        let r = first("Ps 23:1");
        assert_eq!(r.book.as_deref(), Some("Psalms"));
    }

    #[test]
    fn alias_1_cor_abbreviation() {
        let r = first("1 Cor 13:13");
        assert_eq!(r.book.as_deref(), Some("1 Corinthians"));
    }

    #[test]
    fn alias_rev_abbreviation() {
        let r = first("Rev 22:21");
        assert_eq!(r.book.as_deref(), Some("Revelation"));
    }

    #[test]
    fn alias_hab_abbreviation() {
        let r = first("Hab 2:4");
        assert_eq!(r.book.as_deref(), Some("Habakkuk"));
    }

    #[test]
    fn alias_song_of_songs() {
        let r = first("Song of Songs 1:1");
        assert_eq!(r.book.as_deref(), Some("Song of Solomon"));
    }

    #[test]
    fn alias_psalm_singular() {
        let r = first("Psalm 119:1");
        assert_eq!(r.book.as_deref(), Some("Psalms"));
    }

    // ── All 66 books: canonical colon form ───────────────────────────────────

    #[test]
    fn all_66_books_detected_colon_form() {
        use crate::book_data::BOOK_DATA;
        let engine = engine();
        for (canonical, _) in BOOK_DATA {
            let text = format!("{canonical} 1:1");
            let results = engine.find_all(&text);
            assert_eq!(
                results.len(),
                1,
                "expected exactly 1 match for '{text}', got {results:?}"
            );
            assert_eq!(
                results[0].book.as_deref(),
                Some(*canonical),
                "wrong book for '{text}'"
            );
            assert_eq!(results[0].chapter, Some(1));
            assert_eq!(results[0].verse, Some(1));
            assert_eq!(results[0].completeness, MatchCompleteness::FullCanonical);
        }
    }

    #[test]
    fn all_66_books_detected_spoken_form() {
        use crate::book_data::BOOK_DATA;
        let engine = engine();
        for (canonical, _) in BOOK_DATA {
            let text = format!("{canonical} chapter 1 verse 1");
            let results = engine.find_all(&text);
            assert!(!results.is_empty(), "no match for '{text}'");
            assert_eq!(
                results[0].book.as_deref(),
                Some(*canonical),
                "wrong book for spoken form of '{canonical}'"
            );
        }
    }

    // ── Multiple references in one text ──────────────────────────────────────

    #[test]
    fn multiple_references_in_one_text() {
        let results = engine().find_all("From Genesis 1:1 to Revelation 22:21");
        assert_eq!(results.len(), 2, "expected 2 matches, got: {results:?}");
        assert_eq!(results[0].book.as_deref(), Some("Genesis"));
        assert_eq!(results[1].book.as_deref(), Some("Revelation"));
    }

    #[test]
    fn multiple_references_are_sorted_by_position() {
        let results = engine().find_all("Romans 8:1 and then John 3:16");
        assert_eq!(results.len(), 2);
        assert!(results[0].start < results[1].start);
        assert_eq!(results[0].book.as_deref(), Some("Romans"));
        assert_eq!(results[1].book.as_deref(), Some("John"));
    }

    // ── Deduplication: higher priority wins ──────────────────────────────────

    #[test]
    fn dedup_full_ref_beats_book_chapter() {
        // "John 3:16" should be returned as FullCanonical, not also as BookChapter.
        let results = engine().find_all("John 3:16");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].completeness, MatchCompleteness::FullCanonical);
    }

    #[test]
    fn dedup_spoken_beats_book_chapter() {
        let results = engine().find_all("John chapter 3 verse 16");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].completeness, MatchCompleteness::BookChapterVerse);
    }

    #[test]
    fn dedup_spoken_beats_chapter_verse() {
        // "chapter 3 verse 16" inside "John chapter 3 verse 16" must not
        // produce a second ChapterVerse match.
        let results = engine().find_all("John chapter 3 verse 16");
        assert_eq!(results.len(), 1);
    }

    // ── False positive prevention ─────────────────────────────────────────────

    #[test]
    fn no_match_for_name_without_number() {
        none("John went to the store");
    }

    #[test]
    fn no_match_for_name_with_non_reference_continuation() {
        none("Romans said nothing");
        none("Genesis of the idea");
        none("Mark was happy");
    }

    #[test]
    fn no_match_for_verse_keyword_without_number() {
        none("read the verse aloud");
    }

    #[test]
    fn no_match_for_chapter_keyword_alone() {
        none("this chapter is important");
    }

    #[test]
    fn no_match_phone_number_john_333_4444() {
        // "John 333-4444" must NOT produce a BookChapter match.
        let results = engine().find_all("John 333-4444");
        // The phone-number guard prevents "333-4444" from matching as chapter.
        assert!(
            results.is_empty()
                || results
                    .iter()
                    .all(|r| r.completeness != MatchCompleteness::BookChapter),
            "phone number incorrectly matched as book+chapter: {results:?}"
        );
    }

    #[test]
    fn no_match_phone_number_matt_555_1212() {
        let results = engine().find_all("Call Matt 555-1212");
        assert!(
            results.is_empty()
                || results.iter().all(|r| {
                    // If any match exists for a phone, the chapter-dash guard must prevent it
                    r.completeness != MatchCompleteness::BookChapter
                }),
            "phone number matched: {results:?}"
        );
    }

    #[test]
    fn no_match_year_number() {
        // "John 1876" — year-sized number that happens to follow a name.
        // Chapter [1-9]\d{0,2} only matches 1–3 digit numbers, so 1876 won't match.
        let results = engine().find_all("John 1876");
        assert!(results.is_empty(), "year matched: {results:?}");
    }

    #[test]
    fn no_match_zero_chapter() {
        // "[1-9]\d{0,2}" means digits must start with 1-9, so "0" never matches.
        none("John 0:16");
        // "John chapter 0" cannot match; "verse 16" may produce a VerseOnly hit — that's fine.
        let results = engine().find_all("John chapter 0 verse 16");
        assert!(
            results
                .iter()
                .all(|r| r.completeness == MatchCompleteness::VerseOnly),
            "expected only VerseOnly matches (no book-level match for chapter 0), got: {results:?}"
        );
    }

    // ── Confidence values ─────────────────────────────────────────────────────

    #[test]
    fn confidence_full_canonical_is_1_0() {
        assert_eq!(first("John 3:16").confidence, 1.0);
    }

    #[test]
    fn confidence_book_chapter_verse_is_0_95() {
        assert_eq!(first("John chapter 3 verse 16").confidence, 0.95);
    }

    #[test]
    fn confidence_book_chapter_is_0_70() {
        assert_eq!(first("Romans 8").confidence, 0.70);
    }

    #[test]
    fn confidence_chapter_verse_is_0_60() {
        assert_eq!(first("chapter 3 verse 16").confidence, 0.60);
    }

    #[test]
    fn confidence_verse_only_is_0_40() {
        assert_eq!(first("verse 16").confidence, 0.40);
    }

    // ── Case insensitivity ────────────────────────────────────────────────────

    #[test]
    fn case_insensitive_book_name() {
        let r = first("john 3:16");
        assert_eq!(r.book.as_deref(), Some("John"));
    }

    #[test]
    fn case_insensitive_keywords() {
        let r = first("John CHAPTER 3 VERSE 16");
        assert_eq!(r.chapter, Some(3));
        assert_eq!(r.verse, Some(16));
    }

    // ── Start/end byte offsets ────────────────────────────────────────────────

    #[test]
    fn match_offsets_at_start() {
        let text = "John 3:16";
        let r = first(text);
        assert_eq!(r.start, 0);
        assert_eq!(r.end, text.len());
    }

    #[test]
    fn match_offsets_mid_sentence() {
        let text = "We read Romans 8:1 today";
        let r = first(text);
        assert_eq!(&text[r.start..r.end], "Romans 8:1");
    }

    // ── Performance ───────────────────────────────────────────────────────────

    #[test]
    #[ignore = "performance test — run locally with --ignored, flaky on shared CI runners"]
    fn all_patterns_under_5ms() {
        let engine = engine(); // compile once (not counted in budget)
        let text = "Good morning. Turn to the book of John chapter 3 verse 16. \
                    For God so loved the world. Also see Romans 8:1 and \
                    Psalms chapter 23 verse 1. Chapter 1 verse 1 today. \
                    In the beginning, Genesis 1:1, God created. Verse 13 is key. \
                    Open your Bibles to 1 Corinthians 13:13. Habakkuk 2:4.";

        let start = Instant::now();
        for _ in 0..1_000 {
            let _ = engine.find_all(text);
        }
        let per_call_ns = start.elapsed().as_nanos() / 1_000;
        let per_call_ms = per_call_ns as f64 / 1_000_000.0;

        // Each individual call must be well under 5 ms.
        assert!(
            per_call_ms < 5.0,
            "find_all took {per_call_ms:.3} ms on average (budget: 5 ms)"
        );
    }

    #[test]
    #[ignore = "performance test — run locally with --ignored, flaky on shared CI runners"]
    fn single_call_under_10ms() {
        let engine = engine();
        let text = "Turn to John chapter 3 verse 16. Romans 8:1. Psalms 23:1.";
        let t0 = Instant::now();
        let _ = engine.find_all(text);
        let elapsed_ms = t0.elapsed().as_secs_f64() * 1000.0;
        assert!(elapsed_ms < 10.0, "single find_all took {elapsed_ms:.3} ms");
    }
}
