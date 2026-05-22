use std::{collections::HashMap, path::Path};

use companion_errors::BibleError;
use serde::Deserialize;

use crate::search::{score_verse, SearchResult};
use crate::types::{BibleBook, Testament, VerseText};

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
    /// Flat search index: (book_order_0, chapter, verse, lowercase_text).
    /// Built once at load time; avoids allocating per search call.
    search_index: Vec<(u8, u8, u8, String)>,
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

        // Build flat search index in canonical book order.
        let mut search_index: Vec<(u8, u8, u8, String)> = Vec::with_capacity(31_102);
        for (book_idx, book_meta) in meta.iter().enumerate() {
            let chapters = &text[&book_meta.name];
            for (ch_idx, verses) in chapters.iter().enumerate() {
                for (v_idx, verse_text) in verses.iter().enumerate() {
                    if !verse_text.is_empty() {
                        search_index.push((
                            book_idx as u8,
                            (ch_idx + 1) as u8,
                            (v_idx + 1) as u8,
                            verse_text.to_lowercase(),
                        ));
                    }
                }
            }
        }

        Ok(KjvBible {
            text,
            meta,
            meta_index,
            search_index,
        })
    }

    // ── verse access ──────────────────────────────────────────────────────────

    /// Return the text of a single verse, or a `BibleError` if not found.
    pub fn get_verse(&self, book: &str, chapter: u8, verse: u8) -> Result<VerseText, BibleError> {
        let chapters = self
            .text
            .get(book)
            .ok_or_else(|| BibleError::BookNotFound { book: book.into() })?;

        let total_chapters = chapters.len() as u8;
        let ch_idx = chapter
            .checked_sub(1)
            .ok_or_else(|| BibleError::ChapterOutOfRange {
                book: book.into(),
                requested: chapter,
                total: total_chapters,
            })? as usize;

        let verses = chapters
            .get(ch_idx)
            .ok_or_else(|| BibleError::ChapterOutOfRange {
                book: book.into(),
                requested: chapter,
                total: total_chapters,
            })?;

        let total_verses = verses.len() as u8;
        let v_idx = verse
            .checked_sub(1)
            .ok_or_else(|| BibleError::VerseOutOfRange {
                book: book.into(),
                chapter,
                requested: verse,
                total: total_verses,
            })? as usize;

        let text = verses
            .get(v_idx)
            .ok_or_else(|| BibleError::VerseOutOfRange {
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

    // ── full-text search ──────────────────────────────────────────────────────

    /// Search all 31,102 verses for `query` and return results ranked by
    /// relevance (highest score first).
    ///
    /// Matching is case-insensitive substring search.  Score is computed per
    /// verse as:
    ///
    /// * +10 for each query word that appears as a whole word (word boundary)
    /// * +1  for each query word that appears as a substring
    ///
    /// Whole-word matches also count toward the substring score, so a verse
    /// containing all query terms as whole words scores higher than one that
    /// only has substring matches.
    ///
    /// Ties in score are broken by canonical book order (Genesis first).
    ///
    /// Returns an empty `Vec` when no verse contains any query term.
    pub fn search(&self, query: &str) -> Vec<SearchResult> {
        let query = query.trim();
        if query.is_empty() {
            return Vec::new();
        }

        let terms: Vec<String> = query
            .to_lowercase()
            .split_whitespace()
            .filter(|t| !t.is_empty())
            .map(String::from)
            .collect();

        if terms.is_empty() {
            return Vec::new();
        }

        let mut results: Vec<SearchResult> = self
            .search_index
            .iter()
            .filter_map(|(book_idx, chapter, verse, lower)| {
                let score = score_verse(lower, &terms);
                if score == 0 {
                    return None;
                }
                let book_name = &self.meta[*book_idx as usize].name;
                let text = self
                    .text
                    .get(book_name)
                    .and_then(|chs| chs.get(*chapter as usize - 1))
                    .and_then(|vs| vs.get(*verse as usize - 1))
                    .cloned()
                    .unwrap_or_default();
                Some(SearchResult {
                    verse: VerseText {
                        book: book_name.clone(),
                        chapter: *chapter,
                        verse: *verse,
                        text,
                    },
                    score,
                })
            })
            .collect();

        results.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then(self.meta_index[&a.verse.book].cmp(&self.meta_index[&b.verse.book]))
        });
        results
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
        let ch_idx = chapter
            .checked_sub(1)
            .ok_or_else(|| BibleError::ChapterOutOfRange {
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
