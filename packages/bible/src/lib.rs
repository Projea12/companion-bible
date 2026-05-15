use std::{collections::HashMap, path::Path};

use companion_errors::BibleError;
use serde::Deserialize;

// ─── Testament ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Testament {
    OldTestament,
    NewTestament,
}

impl Testament {
    pub fn abbreviation(self) -> &'static str {
        match self {
            Self::OldTestament => "OT",
            Self::NewTestament => "NT",
        }
    }
}

impl std::fmt::Display for Testament {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OldTestament => write!(f, "Old Testament"),
            Self::NewTestament => write!(f, "New Testament"),
        }
    }
}

// ─── BibleBook ────────────────────────────────────────────────────────────────

/// Full metadata for a single Bible book.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BibleBook {
    /// Canonical name exactly as stored in kjv.json (e.g. `"1 Corinthians"`).
    pub name: String,
    pub testament: Testament,
    /// Canonical order: 1 = Genesis … 66 = Revelation.
    pub order: u8,
    /// Number of chapters in this book.
    pub chapter_count: u8,
    /// Total number of verses across all chapters.
    pub verse_count: u32,
}

impl std::fmt::Display for BibleBook {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} ({}, {} ch, {} v)",
            self.name,
            self.testament.abbreviation(),
            self.chapter_count,
            self.verse_count,
        )
    }
}

// ─── BibleReference ───────────────────────────────────────────────────────────

/// A reference to a specific location in the Bible.
///
/// `verse` and `verse_end` are both `None` for chapter-level references.
/// `verse_end` is `Some` only for verse-range references (e.g. John 3:16-17).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BibleReference {
    /// Canonical book name (e.g. `"1 Corinthians"`).
    pub book: String,
    pub chapter: u8,
    pub verse: Option<u8>,
    pub verse_end: Option<u8>,
}

impl BibleReference {
    /// Chapter-level reference: `John 3`.
    pub fn chapter(book: impl Into<String>, chapter: u8) -> Self {
        Self {
            book: book.into(),
            chapter,
            verse: None,
            verse_end: None,
        }
    }

    /// Single-verse reference: `John 3:16`.
    pub fn verse(book: impl Into<String>, chapter: u8, verse: u8) -> Self {
        Self {
            book: book.into(),
            chapter,
            verse: Some(verse),
            verse_end: None,
        }
    }

    /// Verse-range reference: `Romans 8:1-4`.
    pub fn range(book: impl Into<String>, chapter: u8, from: u8, to: u8) -> Self {
        Self {
            book: book.into(),
            chapter,
            verse: Some(from),
            verse_end: Some(to),
        }
    }

    /// `true` if this reference covers a range of verses.
    pub fn is_range(&self) -> bool {
        self.verse_end.is_some()
    }

    /// `true` if this is a chapter-level reference with no verse.
    pub fn is_chapter_ref(&self) -> bool {
        self.verse.is_none()
    }
}

impl std::fmt::Display for BibleReference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.book, self.chapter)?;
        match (self.verse, self.verse_end) {
            (Some(v), Some(end)) => write!(f, ":{v}-{end}"),
            (Some(v), None) => write!(f, ":{v}"),
            (None, _) => Ok(()),
        }
    }
}

// ─── VerseText ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerseText {
    pub book: String,
    pub chapter: u8,
    pub verse: u8,
    pub text: String,
}

impl VerseText {
    /// Produce a `BibleReference` pointing at this verse.
    pub fn reference(&self) -> BibleReference {
        BibleReference::verse(&self.book, self.chapter, self.verse)
    }
}

impl std::fmt::Display for VerseText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} {}:{} — {}",
            self.book, self.chapter, self.verse, self.text
        )
    }
}

// ─── JSON deserialization shapes (private) ────────────────────────────────────

#[derive(Deserialize)]
struct RawVerse {
    verse: u8,
    text: String,
}

#[derive(Deserialize)]
struct RawChapter {
    chapter: u8,
    verses: Vec<RawVerse>,
}

#[derive(Deserialize)]
struct RawBook {
    book: String,
    testament: String, // "OT" or "NT"
    chapters: Vec<RawChapter>,
}

// ─── KjvBible ─────────────────────────────────────────────────────────────────

/// In-memory KJV Bible loaded from `kjv.json`.
///
/// Internal layout: `text[name][chapter_idx][verse_idx]`
/// where `chapter_idx = chapter − 1` and `verse_idx = verse − 1`.
pub struct KjvBible {
    /// Verse text indexed by book name → chapter index → verse index.
    text: HashMap<String, Vec<Vec<String>>>,
    /// Book metadata in canonical Bible order (Genesis … Revelation).
    meta: Vec<BibleBook>,
    /// Book name → index into `meta` for O(1) metadata lookup.
    meta_index: HashMap<String, usize>,
}

impl KjvBible {
    /// Load the Bible from the `kjv.json` produced by the data-preparation step.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, BibleError> {
        let path = path.as_ref();
        let content = std::fs::read_to_string(path).map_err(|e| BibleError::QueryFailed {
            reason: format!("could not read {}: {e}", path.display()),
        })?;
        let raw: Vec<RawBook> =
            serde_json::from_str(&content).map_err(|e| BibleError::QueryFailed {
                reason: format!("JSON parse error: {e}"),
            })?;

        let mut text: HashMap<String, Vec<Vec<String>>> = HashMap::with_capacity(raw.len());
        let mut meta: Vec<BibleBook> = Vec::with_capacity(raw.len());
        let mut meta_index: HashMap<String, usize> = HashMap::with_capacity(raw.len());

        for (order_0, raw_book) in raw.into_iter().enumerate() {
            let testament = match raw_book.testament.as_str() {
                "NT" => Testament::NewTestament,
                _ => Testament::OldTestament,
            };

            let mut chapters: Vec<Vec<String>> = Vec::with_capacity(raw_book.chapters.len());
            let mut total_verses: u32 = 0;

            for raw_chapter in raw_book.chapters {
                let mut verses: Vec<String> = Vec::with_capacity(raw_chapter.verses.len());
                for raw_verse in raw_chapter.verses {
                    let idx = (raw_verse.verse as usize).saturating_sub(1);
                    if idx >= verses.len() {
                        verses.resize(idx + 1, String::new());
                    }
                    verses[idx] = raw_verse.text;
                    total_verses += 1;
                }
                let ch_idx = (raw_chapter.chapter as usize).saturating_sub(1);
                if ch_idx >= chapters.len() {
                    chapters.resize(ch_idx + 1, Vec::new());
                }
                chapters[ch_idx] = verses;
            }

            let book_meta = BibleBook {
                name: raw_book.book.clone(),
                testament,
                order: (order_0 + 1) as u8,
                chapter_count: chapters.len() as u8,
                verse_count: total_verses,
            };

            meta_index.insert(raw_book.book.clone(), meta.len());
            meta.push(book_meta);
            text.insert(raw_book.book, chapters);
        }

        Ok(KjvBible { text, meta, meta_index })
    }

    // ── verse access ──────────────────────────────────────────────────────────

    /// Return the text of a single verse, or a `BibleError` if not found.
    pub fn get_verse(&self, book: &str, chapter: u8, verse: u8) -> Result<VerseText, BibleError> {
        let chapters = self
            .text
            .get(book)
            .ok_or_else(|| BibleError::BookNotFound { book: book.into() })?;

        let total_chapters = chapters.len() as u8;
        let ch_idx = chapter.checked_sub(1).ok_or_else(|| BibleError::ChapterOutOfRange {
            book: book.into(),
            requested: chapter,
            total: total_chapters,
        })? as usize;

        let verses = chapters.get(ch_idx).ok_or_else(|| BibleError::ChapterOutOfRange {
            book: book.into(),
            requested: chapter,
            total: total_chapters,
        })?;

        let total_verses = verses.len() as u8;
        let v_idx = verse.checked_sub(1).ok_or_else(|| BibleError::VerseOutOfRange {
            book: book.into(),
            chapter,
            requested: verse,
            total: total_verses,
        })? as usize;

        let text = verses.get(v_idx).ok_or_else(|| BibleError::VerseOutOfRange {
            book: book.into(),
            chapter,
            requested: verse,
            total: total_verses,
        })?;

        Ok(VerseText {
            book: book.into(),
            chapter,
            verse,
            text: text.clone(),
        })
    }

    // ── book queries ──────────────────────────────────────────────────────────

    /// `true` if the given canonical book name is present in the loaded data.
    pub fn book_exists(&self, name: &str) -> bool {
        self.meta_index.contains_key(name)
    }

    /// Metadata for a single book, or `None` if not found.
    pub fn book_info(&self, name: &str) -> Option<&BibleBook> {
        self.meta_index.get(name).map(|&i| &self.meta[i])
    }

    /// All 66 books in canonical Bible order (Genesis … Revelation).
    pub fn books(&self) -> &[BibleBook] {
        &self.meta
    }

    /// Canonical book names in canonical Bible order.
    pub fn book_names(&self) -> impl Iterator<Item = &str> {
        self.meta.iter().map(|b| b.name.as_str())
    }

    // ── counts ────────────────────────────────────────────────────────────────

    /// Total number of chapters in a book.
    pub fn chapter_count(&self, book: &str) -> Result<u8, BibleError> {
        self.book_info(book)
            .map(|b| b.chapter_count)
            .ok_or_else(|| BibleError::BookNotFound { book: book.into() })
    }

    /// Total number of verses in a specific chapter.
    pub fn verse_count(&self, book: &str, chapter: u8) -> Result<u8, BibleError> {
        let chapters = self
            .text
            .get(book)
            .ok_or_else(|| BibleError::BookNotFound { book: book.into() })?;

        let total_chapters = chapters.len() as u8;
        let ch_idx = chapter.checked_sub(1).ok_or_else(|| BibleError::ChapterOutOfRange {
            book: book.into(),
            requested: chapter,
            total: total_chapters,
        })? as usize;

        chapters
            .get(ch_idx)
            .map(|v| v.len() as u8)
            .ok_or_else(|| BibleError::ChapterOutOfRange {
                book: book.into(),
                requested: chapter,
                total: total_chapters,
            })
    }
}

// ─── ValidationResult ────────────────────────────────────────────────────────

/// The outcome of validating a `BibleReference` against the loaded KJV data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationResult {
    /// The reference is fully valid; contains the resolved verse text.
    Valid(VerseText),

    /// The book name was not found in the KJV (66-book canon).
    InvalidBook { book: String },

    /// The chapter number exceeds the book's actual chapter count,
    /// or is zero.
    InvalidChapter {
        book: String,
        chapter: u8,
        total_chapters: u8,
    },

    /// The verse number exceeds the chapter's actual verse count,
    /// is zero, or was absent from a verse-level reference.
    InvalidVerse {
        book: String,
        chapter: u8,
        verse: u8,
        total_verses: u8,
    },
}

impl ValidationResult {
    /// `true` only when the variant is `Valid`.
    pub fn is_valid(&self) -> bool {
        matches!(self, Self::Valid(_))
    }

    /// Consume the result and return the `VerseText` if valid, otherwise `None`.
    pub fn into_verse(self) -> Option<VerseText> {
        if let Self::Valid(v) = self {
            Some(v)
        } else {
            None
        }
    }
}

impl std::fmt::Display for ValidationResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Valid(v) => write!(f, "{v}"),
            Self::InvalidBook { book } => {
                write!(f, "Unknown book: \"{book}\"")
            }
            Self::InvalidChapter {
                book,
                chapter,
                total_chapters,
            } => write!(
                f,
                "{book} has only {total_chapters} chapter(s) (requested {chapter})"
            ),
            Self::InvalidVerse {
                book,
                chapter,
                verse,
                total_verses,
            } => write!(
                f,
                "{book} {chapter} has only {total_verses} verse(s) (requested {verse})"
            ),
        }
    }
}

// ─── BibleValidator ───────────────────────────────────────────────────────────

/// Validates a `BibleReference` against the loaded KJV data and returns a
/// `ValidationResult` describing exactly why a reference is invalid, or the
/// resolved `VerseText` when it is valid.
///
/// # Chapter-level references
/// If `reference.verse` is `None` the validator cannot resolve verse text.
/// It validates the book and chapter, then returns
/// `InvalidVerse { verse: 0, total_verses }` to indicate no verse was
/// specified.  The caller can distinguish this case by checking `verse == 0`.
pub struct BibleValidator<'a> {
    bible: &'a KjvBible,
}

impl<'a> BibleValidator<'a> {
    pub fn new(bible: &'a KjvBible) -> Self {
        Self { bible }
    }

    pub fn validate(&self, reference: &BibleReference) -> ValidationResult {
        // ── 1. Book ───────────────────────────────────────────────────────────
        if !self.bible.book_exists(&reference.book) {
            return ValidationResult::InvalidBook {
                book: reference.book.clone(),
            };
        }

        // ── 2. Chapter ────────────────────────────────────────────────────────
        // chapter_count is safe to unwrap: we confirmed the book exists above.
        let total_chapters = self.bible.chapter_count(&reference.book).unwrap();
        if reference.chapter == 0 || reference.chapter > total_chapters {
            return ValidationResult::InvalidChapter {
                book: reference.book.clone(),
                chapter: reference.chapter,
                total_chapters,
            };
        }

        // ── 3. Verse ─────────────────────────────────────────────────────────
        // verse_count is safe to unwrap: book + chapter are confirmed valid.
        let total_verses = self
            .bible
            .verse_count(&reference.book, reference.chapter)
            .unwrap();

        let verse = match reference.verse {
            Some(v) => v,
            // Chapter-level reference: no verse to resolve.
            None => {
                return ValidationResult::InvalidVerse {
                    book: reference.book.clone(),
                    chapter: reference.chapter,
                    verse: 0,
                    total_verses,
                }
            }
        };

        if verse == 0 || verse > total_verses {
            return ValidationResult::InvalidVerse {
                book: reference.book.clone(),
                chapter: reference.chapter,
                verse,
                total_verses,
            };
        }

        // ── 4. All valid ──────────────────────────────────────────────────────
        match self.bible.get_verse(&reference.book, reference.chapter, verse) {
            Ok(v) => ValidationResult::Valid(v),
            Err(_) => ValidationResult::InvalidVerse {
                book: reference.book.clone(),
                chapter: reference.chapter,
                verse,
                total_verses,
            },
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
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
        let ot: Vec<_> = books.iter().filter(|b| b.testament == Testament::OldTestament).collect();
        let nt: Vec<_> = books.iter().filter(|b| b.testament == Testament::NewTestament).collect();
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
        assert!(psalms.verse_count > 2_000, "Psalms should have >2000 verses");
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
            "Genesis", "Exodus", "Leviticus", "Numbers", "Deuteronomy",
            "Joshua", "Judges", "Ruth", "1 Samuel", "2 Samuel",
            "1 Kings", "2 Kings", "1 Chronicles", "2 Chronicles", "Ezra",
            "Nehemiah", "Esther", "Job", "Psalms", "Proverbs",
            "Ecclesiastes", "Song of Solomon", "Isaiah", "Jeremiah",
            "Lamentations", "Ezekiel", "Daniel", "Hosea", "Joel", "Amos",
            "Obadiah", "Jonah", "Micah", "Nahum", "Habakkuk",
            "Zephaniah", "Haggai", "Zechariah", "Malachi", "Matthew",
            "Mark", "Luke", "John", "Acts", "Romans",
            "1 Corinthians", "2 Corinthians", "Galatians", "Ephesians",
            "Philippians", "Colossians", "1 Thessalonians", "2 Thessalonians",
            "1 Timothy", "2 Timothy", "Titus", "Philemon", "Hebrews",
            "James", "1 Peter", "2 Peter", "1 John", "2 John", "3 John",
            "Jude", "Revelation",
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
        let result =
            BibleValidator::new(&b).validate(&BibleReference::verse("Revelation", 22, 21));
        assert!(result.is_valid());
    }

    #[test]
    fn validate_is_valid_helper() {
        let b = bible();
        let v = BibleValidator::new(&b);
        assert!(v.validate(&BibleReference::verse("Genesis", 1, 1)).is_valid());
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
        // canonical names are title-cased; lowercase must not match
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
        // John 3 has 36 verses
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
        // chapter-level reference (no verse) → InvalidVerse { verse: 0 }
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
        let result =
            BibleValidator::new(&b).validate(&BibleReference::verse("Hezekiah", 255, 255));
        assert!(matches!(result, ValidationResult::InvalidBook { .. }));
    }

    #[test]
    fn validate_invalid_chapter_takes_priority_over_invalid_verse() {
        let b = bible();
        let result = BibleValidator::new(&b).validate(&BibleReference::verse("Genesis", 255, 255));
        assert!(matches!(result, ValidationResult::InvalidChapter { .. }));
    }
}
