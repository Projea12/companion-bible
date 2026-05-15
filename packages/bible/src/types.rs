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
