use std::{collections::HashMap, path::Path};

use companion_errors::BibleError;
use serde::Deserialize;

// ─── Public types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct VerseText {
    pub book: String,
    pub chapter: u8,
    pub verse: u8,
    pub text: String,
}

impl std::fmt::Display for VerseText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}:{} — {}", self.book, self.chapter, self.verse, self.text)
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
    chapters: Vec<RawChapter>,
}

// ─── KjvBible ─────────────────────────────────────────────────────────────────

/// In-memory KJV Bible loaded from kjv.json.
///
/// Internal layout: `books[name][chapter_idx][verse_idx]`
/// where `chapter_idx = chapter - 1` and `verse_idx = verse - 1`.
pub struct KjvBible {
    books: HashMap<String, Vec<Vec<String>>>,
    book_order: Vec<String>,
}

impl KjvBible {
    /// Load the Bible from a kjv.json file produced by the data-preparation step.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, BibleError> {
        let path = path.as_ref();
        let content = std::fs::read_to_string(path).map_err(|e| BibleError::QueryFailed {
            reason: format!("could not read {}: {e}", path.display()),
        })?;

        let raw: Vec<RawBook> =
            serde_json::from_str(&content).map_err(|e| BibleError::QueryFailed {
                reason: format!("JSON parse error: {e}"),
            })?;

        let mut books: HashMap<String, Vec<Vec<String>>> =
            HashMap::with_capacity(raw.len());
        let mut book_order: Vec<String> = Vec::with_capacity(raw.len());

        for raw_book in raw {
            let mut chapters: Vec<Vec<String>> =
                Vec::with_capacity(raw_book.chapters.len());

            for raw_chapter in raw_book.chapters {
                let mut verses: Vec<String> =
                    Vec::with_capacity(raw_chapter.verses.len());

                for raw_verse in raw_chapter.verses {
                    let idx = (raw_verse.verse as usize).saturating_sub(1);
                    if idx >= verses.len() {
                        verses.resize(idx + 1, String::new());
                    }
                    verses[idx] = raw_verse.text;
                }

                let chapter_idx = (raw_chapter.chapter as usize).saturating_sub(1);
                if chapter_idx >= chapters.len() {
                    chapters.resize(chapter_idx + 1, Vec::new());
                }
                chapters[chapter_idx] = verses;
            }

            book_order.push(raw_book.book.clone());
            books.insert(raw_book.book, chapters);
        }

        Ok(KjvBible { books, book_order })
    }

    /// Return the text of a single verse, or a `BibleError` if the
    /// book / chapter / verse does not exist.
    pub fn get_verse(
        &self,
        book: &str,
        chapter: u8,
        verse: u8,
    ) -> Result<VerseText, BibleError> {
        let chapters = self
            .books
            .get(book)
            .ok_or_else(|| BibleError::BookNotFound { book: book.into() })?;

        let total_chapters = chapters.len() as u8;
        let chapter_idx = chapter.checked_sub(1).ok_or_else(|| {
            BibleError::ChapterOutOfRange {
                book: book.into(),
                requested: chapter,
                total: total_chapters,
            }
        })? as usize;

        let verses = chapters.get(chapter_idx).ok_or_else(|| {
            BibleError::ChapterOutOfRange {
                book: book.into(),
                requested: chapter,
                total: total_chapters,
            }
        })?;

        let total_verses = verses.len() as u8;
        let verse_idx = verse.checked_sub(1).ok_or_else(|| {
            BibleError::VerseOutOfRange {
                book: book.into(),
                chapter,
                requested: verse,
                total: total_verses,
            }
        })? as usize;

        let text = verses.get(verse_idx).ok_or_else(|| {
            BibleError::VerseOutOfRange {
                book: book.into(),
                chapter,
                requested: verse,
                total: total_verses,
            }
        })?;

        Ok(VerseText {
            book: book.into(),
            chapter,
            verse,
            text: text.clone(),
        })
    }

    /// Return `true` if the given canonical book name exists in the loaded data.
    pub fn book_exists(&self, name: &str) -> bool {
        self.books.contains_key(name)
    }

    /// Canonical book names in canonical Bible order.
    pub fn book_names(&self) -> &[String] {
        &self.book_order
    }

    /// Total number of chapters in a book, or `None` if the book is not found.
    pub fn chapter_count(&self, book: &str) -> Option<u8> {
        self.books.get(book).map(|ch| ch.len() as u8)
    }

    /// Total number of verses in a chapter, or `None` if not found.
    pub fn verse_count(&self, book: &str, chapter: u8) -> Option<u8> {
        let idx = chapter.checked_sub(1)? as usize;
        self.books.get(book)?.get(idx).map(|v| v.len() as u8)
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
        let result = KjvBible::load("/nonexistent/path/kjv.json");
        assert!(matches!(result, Err(BibleError::QueryFailed { .. })));
    }

    // ── book_exists ───────────────────────────────────────────────────────────

    #[test]
    fn book_exists_all_66() {
        let b = bible();
        let expected = [
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
        for name in expected {
            assert!(b.book_exists(name), "missing book: {name}");
        }
    }

    #[test]
    fn book_exists_false_for_unknown() {
        let b = bible();
        assert!(!b.book_exists("Hezekiah"));
        assert!(!b.book_exists(""));
        assert!(!b.book_exists("genesis")); // canonical names are title-cased
    }

    #[test]
    fn book_order_starts_with_genesis_ends_with_revelation() {
        let b = bible();
        let names = b.book_names();
        assert_eq!(names.first().map(String::as_str), Some("Genesis"));
        assert_eq!(names.last().map(String::as_str), Some("Revelation"));
        assert_eq!(names.len(), 66);
    }

    // ── get_verse ─────────────────────────────────────────────────────────────

    #[test]
    fn get_verse_genesis_1_1() {
        let b = bible();
        let v = b.get_verse("Genesis", 1, 1).unwrap();
        assert_eq!(v.book, "Genesis");
        assert_eq!(v.chapter, 1);
        assert_eq!(v.verse, 1);
        assert!(
            v.text.contains("In the beginning"),
            "unexpected text: {}",
            v.text
        );
    }

    #[test]
    fn get_verse_john_3_16() {
        let b = bible();
        let v = b.get_verse("John", 3, 16).unwrap();
        assert!(v.text.contains("God so loved"), "unexpected text: {}", v.text);
    }

    #[test]
    fn get_verse_psalm_23_1() {
        let b = bible();
        let v = b.get_verse("Psalms", 23, 1).unwrap();
        assert!(v.text.contains("shepherd"), "unexpected text: {}", v.text);
    }

    #[test]
    fn get_verse_revelation_22_21() {
        let b = bible();
        let v = b.get_verse("Revelation", 22, 21).unwrap();
        assert!(v.text.contains("grace"), "unexpected text: {}", v.text);
    }

    #[test]
    fn get_verse_display_format() {
        let b = bible();
        let v = b.get_verse("John", 3, 16).unwrap();
        let s = v.to_string();
        assert!(s.starts_with("John 3:16 — "));
    }

    #[test]
    fn get_verse_unknown_book_returns_error() {
        let b = bible();
        let result = b.get_verse("Hezekiah", 1, 1);
        assert!(matches!(result, Err(BibleError::BookNotFound { .. })));
    }

    #[test]
    fn get_verse_chapter_zero_returns_error() {
        let b = bible();
        let result = b.get_verse("Genesis", 0, 1);
        assert!(matches!(result, Err(BibleError::ChapterOutOfRange { .. })));
    }

    #[test]
    fn get_verse_chapter_out_of_range_returns_error() {
        let b = bible();
        let result = b.get_verse("Genesis", 255, 1);
        assert!(matches!(result, Err(BibleError::ChapterOutOfRange { .. })));
    }

    #[test]
    fn get_verse_verse_zero_returns_error() {
        let b = bible();
        let result = b.get_verse("Genesis", 1, 0);
        assert!(matches!(result, Err(BibleError::VerseOutOfRange { .. })));
    }

    #[test]
    fn get_verse_verse_out_of_range_returns_error() {
        let b = bible();
        let result = b.get_verse("Genesis", 1, 255);
        assert!(matches!(result, Err(BibleError::VerseOutOfRange { .. })));
    }

    // ── chapter_count / verse_count ───────────────────────────────────────────

    #[test]
    fn chapter_count_psalms_is_150() {
        assert_eq!(bible().chapter_count("Psalms"), Some(150));
    }

    #[test]
    fn chapter_count_unknown_book_is_none() {
        assert_eq!(bible().chapter_count("Hezekiah"), None);
    }

    #[test]
    fn verse_count_john_3_is_36() {
        assert_eq!(bible().verse_count("John", 3), Some(36));
    }

    #[test]
    fn verse_count_genesis_1_is_31() {
        assert_eq!(bible().verse_count("Genesis", 1), Some(31));
    }

    #[test]
    fn verse_count_unknown_chapter_is_none() {
        assert_eq!(bible().verse_count("Genesis", 255), None);
    }
}
