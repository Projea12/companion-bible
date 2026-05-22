mod bible;
mod search;
mod types;
mod validator;

pub use bible::KjvBible;
pub use search::SearchResult;
pub use types::{BibleBook, BibleReference, Testament, VerseText};
pub use validator::{BibleValidator, ValidationResult};

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use companion_errors::BibleError;
    use std::path::PathBuf;

    fn kjv_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/data/kjv.json")
    }

    fn bible() -> KjvBible {
        KjvBible::load(kjv_path()).expect("failed to load kjv.json")
    }

    // ── load ──────────────────────────────────────────────────────────────────

    #[test]
    fn load_succeeds() {
        let _ = bible();
    }

    #[test]
    fn load_wrong_path_returns_error() {
        assert!(matches!(
            KjvBible::load("/nonexistent/kjv.json"),
            Err(BibleError::QueryFailed { .. })
        ));
    }

    // ── Testament ─────────────────────────────────────────────────────────────

    #[test]
    fn testament_abbreviations() {
        assert_eq!(Testament::OldTestament.abbreviation(), "OT");
        assert_eq!(Testament::NewTestament.abbreviation(), "NT");
    }

    #[test]
    fn testament_display() {
        assert_eq!(Testament::OldTestament.to_string(), "Old Testament");
        assert_eq!(Testament::NewTestament.to_string(), "New Testament");
    }

    // ── BibleBook ─────────────────────────────────────────────────────────────

    #[test]
    fn bible_book_display() {
        let b = bible();
        let genesis = b.book_info("Genesis").unwrap();
        let s = genesis.to_string();
        assert!(s.contains("Genesis"), "got: {s}");
        assert!(s.contains("OT"), "got: {s}");
    }

    #[test]
    fn books_returns_66_in_order() {
        let b = bible();
        let books = b.books();
        assert_eq!(books.len(), 66);
        assert_eq!(books[0].name, "Genesis");
        assert_eq!(books[65].name, "Revelation");
    }

    #[test]
    fn books_order_field_is_1_indexed() {
        let b = bible();
        let books = b.books();
        assert_eq!(books[0].order, 1);
        assert_eq!(books[65].order, 66);
        for (i, book) in books.iter().enumerate() {
            assert_eq!(book.order as usize, i + 1, "wrong order for {}", book.name);
        }
    }

    #[test]
    fn books_testament_split_is_correct() {
        let b = bible();
        let books = b.books();
        let ot: Vec<_> = books
            .iter()
            .filter(|b| b.testament == Testament::OldTestament)
            .collect();
        let nt: Vec<_> = books
            .iter()
            .filter(|b| b.testament == Testament::NewTestament)
            .collect();
        assert_eq!(ot.len(), 39, "expected 39 OT books");
        assert_eq!(nt.len(), 27, "expected 27 NT books");
        assert_eq!(ot.last().unwrap().name, "Malachi");
        assert_eq!(nt.first().unwrap().name, "Matthew");
    }

    #[test]
    fn book_info_metadata() {
        let b = bible();
        let psalms = b.book_info("Psalms").unwrap();
        assert_eq!(psalms.chapter_count, 150);
        assert_eq!(psalms.testament, Testament::OldTestament);
        assert_eq!(psalms.order, 19);
        assert!(
            psalms.verse_count > 2_000,
            "Psalms should have >2000 verses"
        );
    }

    #[test]
    fn book_info_unknown_returns_none() {
        assert!(bible().book_info("Hezekiah").is_none());
    }

    // ── BibleReference ────────────────────────────────────────────────────────

    #[test]
    fn bible_reference_chapter_display() {
        let r = BibleReference::chapter("John", 3);
        assert_eq!(r.to_string(), "John 3");
    }

    #[test]
    fn bible_reference_verse_display() {
        let r = BibleReference::verse("John", 3, 16);
        assert_eq!(r.to_string(), "John 3:16");
    }

    #[test]
    fn bible_reference_range_display() {
        let r = BibleReference::range("Romans", 8, 1, 4);
        assert_eq!(r.to_string(), "Romans 8:1-4");
    }

    #[test]
    fn bible_reference_is_range() {
        assert!(!BibleReference::verse("John", 3, 16).is_range());
        assert!(BibleReference::range("John", 3, 16, 17).is_range());
    }

    #[test]
    fn bible_reference_is_chapter_ref() {
        assert!(BibleReference::chapter("John", 3).is_chapter_ref());
        assert!(!BibleReference::verse("John", 3, 16).is_chapter_ref());
    }

    #[test]
    fn verse_text_reference_round_trip() {
        let b = bible();
        let v = b.get_verse("John", 3, 16).unwrap();
        let r = v.reference();
        assert_eq!(r.to_string(), "John 3:16");
    }

    // ── book_exists ───────────────────────────────────────────────────────────

    #[test]
    fn book_exists_all_66() {
        let b = bible();
        let names = [
            "Genesis",
            "Exodus",
            "Leviticus",
            "Numbers",
            "Deuteronomy",
            "Joshua",
            "Judges",
            "Ruth",
            "1 Samuel",
            "2 Samuel",
            "1 Kings",
            "2 Kings",
            "1 Chronicles",
            "2 Chronicles",
            "Ezra",
            "Nehemiah",
            "Esther",
            "Job",
            "Psalms",
            "Proverbs",
            "Ecclesiastes",
            "Song of Solomon",
            "Isaiah",
            "Jeremiah",
            "Lamentations",
            "Ezekiel",
            "Daniel",
            "Hosea",
            "Joel",
            "Amos",
            "Obadiah",
            "Jonah",
            "Micah",
            "Nahum",
            "Habakkuk",
            "Zephaniah",
            "Haggai",
            "Zechariah",
            "Malachi",
            "Matthew",
            "Mark",
            "Luke",
            "John",
            "Acts",
            "Romans",
            "1 Corinthians",
            "2 Corinthians",
            "Galatians",
            "Ephesians",
            "Philippians",
            "Colossians",
            "1 Thessalonians",
            "2 Thessalonians",
            "1 Timothy",
            "2 Timothy",
            "Titus",
            "Philemon",
            "Hebrews",
            "James",
            "1 Peter",
            "2 Peter",
            "1 John",
            "2 John",
            "3 John",
            "Jude",
            "Revelation",
        ];
        for name in names {
            assert!(b.book_exists(name), "missing: {name}");
        }
    }

    #[test]
    fn book_exists_false_for_unknown() {
        let b = bible();
        assert!(!b.book_exists("Hezekiah"));
        assert!(!b.book_exists(""));
        assert!(!b.book_exists("genesis"));
    }

    // ── get_verse ─────────────────────────────────────────────────────────────

    #[test]
    fn get_verse_genesis_1_1() {
        let v = bible().get_verse("Genesis", 1, 1).unwrap();
        assert_eq!(v.book, "Genesis");
        assert_eq!(v.chapter, 1);
        assert_eq!(v.verse, 1);
        assert!(v.text.contains("In the beginning"), "{}", v.text);
    }

    #[test]
    fn get_verse_john_3_16() {
        let v = bible().get_verse("John", 3, 16).unwrap();
        assert!(v.text.contains("God so loved"), "{}", v.text);
    }

    #[test]
    fn get_verse_psalm_23_1() {
        let v = bible().get_verse("Psalms", 23, 1).unwrap();
        assert!(v.text.contains("shepherd"), "{}", v.text);
    }

    #[test]
    fn get_verse_revelation_22_21() {
        let v = bible().get_verse("Revelation", 22, 21).unwrap();
        assert!(v.text.contains("grace"), "{}", v.text);
    }

    #[test]
    fn get_verse_display_format() {
        let v = bible().get_verse("John", 3, 16).unwrap();
        assert!(v.to_string().starts_with("John 3:16 — "));
    }

    #[test]
    fn get_verse_unknown_book() {
        assert!(matches!(
            bible().get_verse("Hezekiah", 1, 1),
            Err(BibleError::BookNotFound { .. })
        ));
    }

    #[test]
    fn get_verse_chapter_zero() {
        assert!(matches!(
            bible().get_verse("Genesis", 0, 1),
            Err(BibleError::ChapterOutOfRange { .. })
        ));
    }

    #[test]
    fn get_verse_chapter_out_of_range() {
        assert!(matches!(
            bible().get_verse("Genesis", 255, 1),
            Err(BibleError::ChapterOutOfRange { .. })
        ));
    }

    #[test]
    fn get_verse_verse_zero() {
        assert!(matches!(
            bible().get_verse("Genesis", 1, 0),
            Err(BibleError::VerseOutOfRange { .. })
        ));
    }

    #[test]
    fn get_verse_verse_out_of_range() {
        assert!(matches!(
            bible().get_verse("Genesis", 1, 255),
            Err(BibleError::VerseOutOfRange { .. })
        ));
    }

    // ── chapter_count ─────────────────────────────────────────────────────────

    #[test]
    fn chapter_count_psalms_is_150() {
        assert_eq!(bible().chapter_count("Psalms").unwrap(), 150);
    }

    #[test]
    fn chapter_count_genesis_is_50() {
        assert_eq!(bible().chapter_count("Genesis").unwrap(), 50);
    }

    #[test]
    fn chapter_count_obadiah_is_1() {
        assert_eq!(bible().chapter_count("Obadiah").unwrap(), 1);
    }

    #[test]
    fn chapter_count_unknown_book_returns_error() {
        assert!(matches!(
            bible().chapter_count("Hezekiah"),
            Err(BibleError::BookNotFound { .. })
        ));
    }

    // ── verse_count ───────────────────────────────────────────────────────────

    #[test]
    fn verse_count_john_3_is_36() {
        assert_eq!(bible().verse_count("John", 3).unwrap(), 36);
    }

    #[test]
    fn verse_count_genesis_1_is_31() {
        assert_eq!(bible().verse_count("Genesis", 1).unwrap(), 31);
    }

    #[test]
    fn verse_count_unknown_book_returns_error() {
        assert!(matches!(
            bible().verse_count("Hezekiah", 1),
            Err(BibleError::BookNotFound { .. })
        ));
    }

    #[test]
    fn verse_count_chapter_out_of_range_returns_error() {
        assert!(matches!(
            bible().verse_count("Genesis", 255),
            Err(BibleError::ChapterOutOfRange { .. })
        ));
    }

    #[test]
    fn verse_count_chapter_zero_returns_error() {
        assert!(matches!(
            bible().verse_count("Genesis", 0),
            Err(BibleError::ChapterOutOfRange { .. })
        ));
    }

    // ── BibleValidator — Valid ────────────────────────────────────────────────

    #[test]
    fn validate_valid_verse_returns_verse_text() {
        let b = bible();
        let v = BibleValidator::new(&b);
        let r = BibleReference::verse("John", 3, 16);
        assert!(matches!(v.validate(&r), ValidationResult::Valid(_)));
    }

    #[test]
    fn validate_valid_verse_text_is_correct() {
        let b = bible();
        let result = BibleValidator::new(&b).validate(&BibleReference::verse("John", 3, 16));
        let verse = result.into_verse().expect("should be valid");
        assert!(verse.text.contains("God so loved"), "{}", verse.text);
        assert_eq!(verse.book, "John");
        assert_eq!(verse.chapter, 3);
        assert_eq!(verse.verse, 16);
    }

    #[test]
    fn validate_valid_genesis_1_1() {
        let b = bible();
        let result = BibleValidator::new(&b).validate(&BibleReference::verse("Genesis", 1, 1));
        assert!(result.is_valid());
    }

    #[test]
    fn validate_valid_revelation_22_21() {
        let b = bible();
        let result = BibleValidator::new(&b).validate(&BibleReference::verse("Revelation", 22, 21));
        assert!(result.is_valid());
    }

    #[test]
    fn validate_is_valid_helper() {
        let b = bible();
        let v = BibleValidator::new(&b);
        assert!(v
            .validate(&BibleReference::verse("Genesis", 1, 1))
            .is_valid());
        assert!(!v
            .validate(&BibleReference::verse("Hezekiah", 1, 1))
            .is_valid());
    }

    // ── BibleValidator — InvalidBook ──────────────────────────────────────────

    #[test]
    fn validate_invalid_book_unknown_name() {
        let b = bible();
        let result = BibleValidator::new(&b).validate(&BibleReference::verse("Hezekiah", 1, 1));
        assert!(matches!(
            result,
            ValidationResult::InvalidBook { book } if book == "Hezekiah"
        ));
    }

    #[test]
    fn validate_invalid_book_lowercase_name() {
        let b = bible();
        let result = BibleValidator::new(&b).validate(&BibleReference::verse("genesis", 1, 1));
        assert!(matches!(result, ValidationResult::InvalidBook { .. }));
    }

    #[test]
    fn validate_invalid_book_empty_string() {
        let b = bible();
        let result = BibleValidator::new(&b).validate(&BibleReference::verse("", 1, 1));
        assert!(matches!(result, ValidationResult::InvalidBook { .. }));
    }

    #[test]
    fn validate_invalid_book_display() {
        let b = bible();
        let result = BibleValidator::new(&b).validate(&BibleReference::verse("Hezekiah", 1, 1));
        assert!(result.to_string().contains("Hezekiah"), "{result}");
    }

    // ── BibleValidator — InvalidChapter ───────────────────────────────────────

    #[test]
    fn validate_invalid_chapter_zero() {
        let b = bible();
        let result = BibleValidator::new(&b).validate(&BibleReference::verse("Genesis", 0, 1));
        assert!(matches!(
            result,
            ValidationResult::InvalidChapter { chapter: 0, .. }
        ));
    }

    #[test]
    fn validate_invalid_chapter_exceeds_book_length() {
        let b = bible();
        let result = BibleValidator::new(&b).validate(&BibleReference::verse("Genesis", 51, 1));
        assert!(matches!(
            result,
            ValidationResult::InvalidChapter {
                total_chapters: 50,
                chapter: 51,
                ..
            }
        ));
    }

    #[test]
    fn validate_invalid_chapter_contains_correct_total() {
        let b = bible();
        let result = BibleValidator::new(&b).validate(&BibleReference::verse("Obadiah", 2, 1));
        assert!(matches!(
            result,
            ValidationResult::InvalidChapter {
                total_chapters: 1,
                ..
            }
        ));
    }

    #[test]
    fn validate_invalid_chapter_display() {
        let b = bible();
        let result = BibleValidator::new(&b).validate(&BibleReference::verse("Genesis", 51, 1));
        let s = result.to_string();
        assert!(s.contains("Genesis"), "{s}");
        assert!(s.contains("50"), "{s}");
        assert!(s.contains("51"), "{s}");
    }

    // ── BibleValidator — InvalidVerse ─────────────────────────────────────────

    #[test]
    fn validate_invalid_verse_zero() {
        let b = bible();
        let result = BibleValidator::new(&b).validate(&BibleReference::verse("Genesis", 1, 0));
        assert!(matches!(
            result,
            ValidationResult::InvalidVerse { verse: 0, .. }
        ));
    }

    #[test]
    fn validate_invalid_verse_exceeds_chapter_length() {
        let b = bible();
        let result = BibleValidator::new(&b).validate(&BibleReference::verse("Genesis", 1, 255));
        assert!(matches!(
            result,
            ValidationResult::InvalidVerse { verse: 255, .. }
        ));
    }

    #[test]
    fn validate_invalid_verse_contains_correct_total() {
        let b = bible();
        let result = BibleValidator::new(&b).validate(&BibleReference::verse("John", 3, 37));
        assert!(matches!(
            result,
            ValidationResult::InvalidVerse {
                total_verses: 36,
                verse: 37,
                ..
            }
        ));
    }

    #[test]
    fn validate_invalid_verse_display() {
        let b = bible();
        let result = BibleValidator::new(&b).validate(&BibleReference::verse("John", 3, 37));
        let s = result.to_string();
        assert!(s.contains("John"), "{s}");
        assert!(s.contains("36"), "{s}");
        assert!(s.contains("37"), "{s}");
    }

    #[test]
    fn validate_chapter_level_reference_returns_invalid_verse_zero() {
        let b = bible();
        let result = BibleValidator::new(&b).validate(&BibleReference::chapter("John", 3));
        assert!(matches!(
            result,
            ValidationResult::InvalidVerse { verse: 0, .. }
        ));
    }

    // ── BibleValidator — error priority order ─────────────────────────────────

    #[test]
    fn validate_invalid_book_takes_priority_over_invalid_chapter() {
        let b = bible();
        let result = BibleValidator::new(&b).validate(&BibleReference::verse("Hezekiah", 255, 255));
        assert!(matches!(result, ValidationResult::InvalidBook { .. }));
    }

    #[test]
    fn validate_invalid_chapter_takes_priority_over_invalid_verse() {
        let b = bible();
        let result = BibleValidator::new(&b).validate(&BibleReference::verse("Genesis", 255, 255));
        assert!(matches!(result, ValidationResult::InvalidChapter { .. }));
    }

    // ─── Full-text search — accuracy ─────────────────────────────────────────

    #[test]
    fn search_empty_query_returns_nothing() {
        let b = bible();
        assert!(b.search("").is_empty());
        assert!(b.search("   ").is_empty());
    }

    #[test]
    fn search_unknown_word_returns_nothing() {
        let b = bible();
        assert!(b.search("xyzzy").is_empty());
    }

    #[test]
    fn search_john_3_16_top_result() {
        let b = bible();
        let results = b.search("For God so loved the world");
        assert!(!results.is_empty(), "expected at least one result");
        let top = &results[0];
        assert_eq!(top.verse.book, "John");
        assert_eq!(top.verse.chapter, 3);
        assert_eq!(top.verse.verse, 16);
    }

    #[test]
    fn search_psalm_23_1_top_result() {
        let b = bible();
        let results = b.search("The LORD is my shepherd");
        assert!(!results.is_empty());
        let top = &results[0];
        assert_eq!(top.verse.book, "Psalms");
        assert_eq!(top.verse.chapter, 23);
        assert_eq!(top.verse.verse, 1);
    }

    #[test]
    fn search_beginning_was_the_word_top_result() {
        let b = bible();
        let results = b.search("In the beginning was the Word");
        assert!(!results.is_empty());
        let top = &results[0];
        assert_eq!(top.verse.book, "John");
        assert_eq!(top.verse.chapter, 1);
        assert_eq!(top.verse.verse, 1);
    }

    #[test]
    fn search_case_insensitive() {
        let b = bible();
        let upper = b.search("LOVE");
        let lower = b.search("love");
        assert_eq!(
            upper.len(),
            lower.len(),
            "case should not affect result count"
        );
    }

    #[test]
    fn search_partial_word_matches() {
        let b = bible();
        let results = b.search("loveth");
        assert!(!results.is_empty(), "partial word should match");
    }

    #[test]
    fn search_results_sorted_highest_score_first() {
        let b = bible();
        let results = b.search("love");
        for window in results.windows(2) {
            assert!(
                window[0].score >= window[1].score,
                "results must be sorted descending by score"
            );
        }
    }

    #[test]
    fn search_multi_term_scores_higher_than_single_term() {
        let b = bible();
        let single = b.search("love");
        let multi = b.search("God is love");
        let max_single = single.iter().map(|r| r.score).max().unwrap_or(0);
        let max_multi = multi.iter().map(|r| r.score).max().unwrap_or(0);
        assert!(
            max_multi > max_single,
            "multi-term query should yield higher max score"
        );
    }

    #[test]
    fn search_god_is_love_returns_1_john_4_8() {
        let b = bible();
        let results = b.search("God is love");
        assert!(!results.is_empty());
        let found = results
            .iter()
            .any(|r| r.verse.book == "1 John" && r.verse.chapter == 4 && r.verse.verse == 8);
        assert!(found, "1 John 4:8 must appear in results for 'God is love'");
    }

    #[test]
    fn search_single_char_query_matches() {
        let b = bible();
        let results = b.search("a");
        assert!(!results.is_empty());
    }

    #[test]
    fn search_score_struct_fields_populated() {
        let b = bible();
        let results = b.search("grace");
        assert!(!results.is_empty());
        let first = &results[0];
        assert!(!first.verse.book.is_empty());
        assert!(first.verse.chapter >= 1);
        assert!(first.verse.verse >= 1);
        assert!(!first.verse.text.is_empty());
        assert!(first.score > 0);
    }

    // ─── Full-text search — performance ──────────────────────────────────────
    // Timing assertions are only enforced in release builds (`cargo test
    // --release`).  Debug builds skip the deadline to avoid false failures
    // from the lack of compiler optimisations.

    #[test]
    #[allow(unused_variables)]
    fn search_single_word_under_50ms() {
        let b = bible();
        let start = std::time::Instant::now();
        let results = b.search("love");
        let elapsed = start.elapsed();
        assert!(!results.is_empty());
        #[cfg(not(debug_assertions))]
        assert!(
            elapsed.as_millis() < 50,
            "search took {}ms, must be under 50ms",
            elapsed.as_millis()
        );
    }

    #[test]
    #[allow(unused_variables)]
    fn search_phrase_under_50ms() {
        let b = bible();
        let start = std::time::Instant::now();
        let results = b.search("For God so loved the world");
        let elapsed = start.elapsed();
        assert!(!results.is_empty());
        #[cfg(not(debug_assertions))]
        assert!(
            elapsed.as_millis() < 50,
            "phrase search took {}ms, must be under 50ms",
            elapsed.as_millis()
        );
    }

    #[test]
    #[allow(unused_variables)]
    fn search_rare_term_under_50ms() {
        let b = bible();
        let start = std::time::Instant::now();
        let results = b.search("Melchizedek");
        let elapsed = start.elapsed();
        assert!(!results.is_empty());
        #[cfg(not(debug_assertions))]
        assert!(
            elapsed.as_millis() < 50,
            "rare-term search took {}ms, must be under 50ms",
            elapsed.as_millis()
        );
    }

    #[test]
    #[allow(unused_variables)]
    fn search_no_match_under_50ms() {
        let b = bible();
        let start = std::time::Instant::now();
        let results = b.search("xyzzy");
        let elapsed = start.elapsed();
        assert!(results.is_empty());
        #[cfg(not(debug_assertions))]
        assert!(
            elapsed.as_millis() < 50,
            "no-match search took {}ms, must be under 50ms",
            elapsed.as_millis()
        );
    }

    #[test]
    #[allow(unused_variables)]
    fn search_high_frequency_word_under_50ms() {
        // "the" appears in almost every verse — worst-case scan
        let b = bible();
        let start = std::time::Instant::now();
        let results = b.search("the");
        let elapsed = start.elapsed();
        assert!(!results.is_empty());
        #[cfg(not(debug_assertions))]
        assert!(
            elapsed.as_millis() < 50,
            "high-frequency search took {}ms, must be under 50ms",
            elapsed.as_millis()
        );
    }

    // ─── Property tests ───────────────────────────────────────────────────────
    //
    // These tests use `proptest` to generate random inputs and verify
    // invariants that must hold for ALL possible references.

    mod property {
        use super::*;
        use proptest::prelude::*;

        /// Lazy-loaded KJV bible shared across all property test runs.
        fn bible() -> KjvBible {
            KjvBible::load(kjv_path()).expect("failed to load kjv.json")
        }

        /// Strategy: pick a random valid book by index, then random valid
        /// chapter and verse within that book's actual bounds.
        #[allow(dead_code)]
        fn valid_reference_strategy(b: &KjvBible) -> impl Strategy<Value = BibleReference> {
            let books: Vec<(String, u8)> = b
                .books()
                .iter()
                .map(|book| (book.name.clone(), book.chapter_count))
                .collect();

            // Build the full list of (book, chapter, max_verse) triples.
            let mut triples: Vec<(String, u8, u8)> = Vec::new();
            for (name, chapters) in &books {
                for ch in 1..=*chapters {
                    let max_v = b.verse_count(name, ch).unwrap();
                    triples.push((name.clone(), ch, max_v));
                }
            }

            let idx = 0..triples.len();
            idx.prop_flat_map(move |i| {
                let (name, ch, max_v) = triples[i].clone();
                (1u8..=max_v).prop_map(move |v| BibleReference::verse(&name, ch, v))
            })
        }

        proptest! {
            /// Every reference drawn from the valid-reference strategy must
            /// return `ValidationResult::Valid`.
            #[test]
            fn valid_references_always_return_valid(
                idx in 0usize..31102  // upper bound slightly over total verses — clamped inside
            ) {
                let b = bible();
                let books = b.books();

                // Map flat index to (book, chapter, verse).
                let mut remaining = idx % 31102;
                let mut found = None;
                'outer: for book in books {
                    for ch in 1..=book.chapter_count {
                        let max_v = b.verse_count(&book.name, ch).unwrap();
                        if remaining < max_v as usize {
                            found = Some(BibleReference::verse(&book.name, ch, (remaining + 1) as u8));
                            break 'outer;
                        }
                        remaining -= max_v as usize;
                    }
                }

                if let Some(r) = found {
                    let result = BibleValidator::new(&b).validate(&r);
                    prop_assert!(
                        result.is_valid(),
                        "expected Valid for {}, got: {}",
                        r,
                        result
                    );
                }
            }

            /// A reference with an unknown book name always returns `InvalidBook`.
            #[test]
            fn unknown_book_always_returns_invalid_book(
                // Generate strings that are guaranteed not to be canonical book names.
                suffix in "[0-9]{4,8}"
            ) {
                let b = bible();
                let fake_book = format!("FakeBook{suffix}");
                let r = BibleReference::verse(&fake_book, 1, 1);
                let result = BibleValidator::new(&b).validate(&r);
                prop_assert!(
                    matches!(result, ValidationResult::InvalidBook { .. }),
                    "expected InvalidBook for book={fake_book}, got: {result}"
                );
            }

            /// A chapter of 0 always returns `InvalidChapter` for any valid book.
            #[test]
            fn chapter_zero_always_returns_invalid_chapter(
                book_idx in 0usize..66
            ) {
                let b = bible();
                let book = &b.books()[book_idx].name.clone();
                let r = BibleReference::verse(book, 0, 1);
                let result = BibleValidator::new(&b).validate(&r);
                prop_assert!(
                    matches!(result, ValidationResult::InvalidChapter { chapter: 0, .. }),
                    "expected InvalidChapter(chapter=0) for {book}, got: {result}"
                );
            }

            /// A chapter beyond the book's total always returns `InvalidChapter`.
            #[test]
            fn chapter_beyond_max_always_returns_invalid_chapter(
                book_idx in 0usize..66,
                overflow in 1u8..=20
            ) {
                let b = bible();
                let book_meta = &b.books()[book_idx];
                let book = book_meta.name.clone();
                let bad_chapter = book_meta.chapter_count.saturating_add(overflow);
                // Skip if addition wrapped (u8 overflow — extremely unlikely but safe).
                prop_assume!(bad_chapter > book_meta.chapter_count);

                let r = BibleReference::verse(&book, bad_chapter, 1);
                let result = BibleValidator::new(&b).validate(&r);
                prop_assert!(
                    matches!(result, ValidationResult::InvalidChapter { .. }),
                    "expected InvalidChapter for {book} ch {bad_chapter}, got: {result}"
                );
            }

            /// A verse of 0 for any valid book+chapter always returns `InvalidVerse`.
            #[test]
            fn verse_zero_always_returns_invalid_verse(
                book_idx in 0usize..66,
                chapter_offset in 0u8..10
            ) {
                let b = bible();
                let book_meta = &b.books()[book_idx];
                let book = book_meta.name.clone();
                let ch = (chapter_offset % book_meta.chapter_count) + 1;

                let r = BibleReference::verse(&book, ch, 0);
                let result = BibleValidator::new(&b).validate(&r);
                prop_assert!(
                    matches!(result, ValidationResult::InvalidVerse { verse: 0, .. }),
                    "expected InvalidVerse(verse=0) for {book} {ch}:0, got: {result}"
                );
            }

            /// A verse beyond the chapter's total always returns `InvalidVerse`.
            #[test]
            fn verse_beyond_max_always_returns_invalid_verse(
                book_idx in 0usize..66,
                chapter_offset in 0u8..10,
                overflow in 1u8..=20
            ) {
                let b = bible();
                let book_meta = &b.books()[book_idx];
                let book = book_meta.name.clone();
                let ch = (chapter_offset % book_meta.chapter_count) + 1;
                let max_v = b.verse_count(&book, ch).unwrap();
                let bad_verse = max_v.saturating_add(overflow);
                prop_assume!(bad_verse > max_v);

                let r = BibleReference::verse(&book, ch, bad_verse);
                let result = BibleValidator::new(&b).validate(&r);
                prop_assert!(
                    matches!(result, ValidationResult::InvalidVerse { .. }),
                    "expected InvalidVerse for {book} {ch}:{bad_verse}, got: {result}"
                );
            }
        }
    }

    // ─── Boundary conditions ──────────────────────────────────────────────────

    #[test]
    fn boundary_genesis_1_1_is_valid() {
        let b = bible();
        let result = BibleValidator::new(&b).validate(&BibleReference::verse("Genesis", 1, 1));
        assert!(result.is_valid(), "Genesis 1:1 should be valid");
    }

    #[test]
    fn boundary_revelation_22_21_is_valid() {
        let b = bible();
        let result = BibleValidator::new(&b).validate(&BibleReference::verse("Revelation", 22, 21));
        assert!(result.is_valid(), "Revelation 22:21 should be valid");
    }

    #[test]
    fn boundary_chapter_1_verse_1_of_every_book() {
        let b = bible();
        for book in b.book_names() {
            let result = BibleValidator::new(&b).validate(&BibleReference::verse(book, 1, 1));
            assert!(
                result.is_valid(),
                "{book} 1:1 should be valid, got: {result}"
            );
        }
    }

    #[test]
    fn boundary_last_verse_of_every_book() {
        let b = bible();
        let cases: &[(&str, u8, u8)] = &[
            ("Genesis", 50, 26),
            ("Exodus", 40, 38),
            ("Leviticus", 27, 34),
            ("Numbers", 36, 13),
            ("Deuteronomy", 34, 12),
            ("Joshua", 24, 33),
            ("Judges", 21, 25),
            ("Ruth", 4, 22),
            ("1 Samuel", 31, 13),
            ("2 Samuel", 24, 25),
            ("1 Kings", 22, 53),
            ("2 Kings", 25, 30),
            ("1 Chronicles", 29, 30),
            ("2 Chronicles", 36, 23),
            ("Ezra", 10, 44),
            ("Nehemiah", 13, 31),
            ("Esther", 10, 3),
            ("Job", 42, 17),
            ("Psalms", 150, 6),
            ("Proverbs", 31, 31),
            ("Ecclesiastes", 12, 14),
            ("Song of Solomon", 8, 14),
            ("Isaiah", 66, 24),
            ("Jeremiah", 52, 34),
            ("Lamentations", 5, 22),
            ("Ezekiel", 48, 35),
            ("Daniel", 12, 13),
            ("Hosea", 14, 9),
            ("Joel", 3, 21),
            ("Amos", 9, 15),
            ("Obadiah", 1, 21),
            ("Jonah", 4, 11),
            ("Micah", 7, 20),
            ("Nahum", 3, 19),
            ("Habakkuk", 3, 19),
            ("Zephaniah", 3, 20),
            ("Haggai", 2, 23),
            ("Zechariah", 14, 21),
            ("Malachi", 4, 6),
            ("Matthew", 28, 20),
            ("Mark", 16, 20),
            ("Luke", 24, 53),
            ("John", 21, 25),
            ("Acts", 28, 31),
            ("Romans", 16, 27),
            ("1 Corinthians", 16, 24),
            ("2 Corinthians", 13, 14),
            ("Galatians", 6, 18),
            ("Ephesians", 6, 24),
            ("Philippians", 4, 23),
            ("Colossians", 4, 18),
            ("1 Thessalonians", 5, 28),
            ("2 Thessalonians", 3, 18),
            ("1 Timothy", 6, 21),
            ("2 Timothy", 4, 22),
            ("Titus", 3, 15),
            ("Philemon", 1, 25),
            ("Hebrews", 13, 25),
            ("James", 5, 20),
            ("1 Peter", 5, 14),
            ("2 Peter", 3, 18),
            ("1 John", 5, 21),
            ("2 John", 1, 13),
            ("3 John", 1, 14),
            ("Jude", 1, 25),
            ("Revelation", 22, 21),
        ];
        for &(book, chapter, verse) in cases {
            let result =
                BibleValidator::new(&b).validate(&BibleReference::verse(book, chapter, verse));
            assert!(
                result.is_valid(),
                "{book} {chapter}:{verse} should be valid, got: {result}"
            );
        }
    }
}
