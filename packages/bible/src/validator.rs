use crate::bible::KjvBible;
use crate::types::{BibleReference, VerseText};

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
        match self
            .bible
            .get_verse(&reference.book, reference.chapter, verse)
        {
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
