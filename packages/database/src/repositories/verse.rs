use companion_errors::DatabaseError;

use crate::{models::Verse, DbPool};

// ─── FtsResult ────────────────────────────────────────────────────────────────

/// A verse returned by an FTS5 quotation search, with its BM25 rank.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct FtsResult {
    pub book: String,
    pub chapter: i64,
    pub verse_number: i64,
    pub text: String,
    /// BM25 score — lower (more negative) means better match.
    pub rank: f64,
}

// ─── VerseRepository ──────────────────────────────────────────────────────────

pub struct VerseRepository {
    pool: DbPool,
}

impl VerseRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    pub async fn get_by_reference(
        &self,
        book: &str,
        chapter: i64,
        verse: i64,
    ) -> Result<Option<Verse>, DatabaseError> {
        sqlx::query_as("SELECT * FROM verses WHERE book = ? AND chapter = ? AND verse_number = ?")
            .bind(book)
            .bind(chapter)
            .bind(verse)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed {
                reason: e.to_string(),
            })
    }

    pub async fn search_full_text(&self, query: &str) -> Result<Vec<Verse>, DatabaseError> {
        let pattern = format!("%{query}%");
        sqlx::query_as(
            "SELECT * FROM verses WHERE text LIKE ?
             ORDER BY book_order, chapter, verse_number",
        )
        .bind(pattern)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryFailed {
            reason: e.to_string(),
        })
    }

    pub async fn get_all_for_book(&self, book: &str) -> Result<Vec<Verse>, DatabaseError> {
        sqlx::query_as("SELECT * FROM verses WHERE book = ? ORDER BY chapter, verse_number")
            .bind(book)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed {
                reason: e.to_string(),
            })
    }

    /// Search the FTS5 index for verses whose text best matches `transcript`.
    ///
    /// Optionally narrow to a specific `book` and `chapter` when the sermon
    /// context is already known — this dramatically improves precision.
    ///
    /// Returns up to `limit` candidates ranked by BM25 relevance.
    pub async fn search_fts(
        &self,
        transcript: &str,
        book: Option<&str>,
        chapter: Option<u8>,
        limit: i64,
    ) -> Result<Vec<FtsResult>, DatabaseError> {
        let query = build_fts_query(transcript);
        if query.is_empty() {
            return Ok(vec![]);
        }

        let results = match (book, chapter) {
            (Some(b), Some(ch)) => {
                sqlx::query_as::<_, FtsResult>(
                    "SELECT f.book, f.chapter, f.verse_number, f.text, f.rank
                     FROM verses_fts f
                     JOIN verses v ON v.id = f.rowid
                     WHERE verses_fts MATCH ?
                       AND f.book = ?
                       AND f.chapter = ?
                     ORDER BY f.rank
                     LIMIT ?",
                )
                .bind(&query)
                .bind(b)
                .bind(ch as i64)
                .bind(limit)
                .fetch_all(&self.pool)
                .await
            }
            (Some(b), None) => {
                sqlx::query_as::<_, FtsResult>(
                    "SELECT f.book, f.chapter, f.verse_number, f.text, f.rank
                     FROM verses_fts f
                     JOIN verses v ON v.id = f.rowid
                     WHERE verses_fts MATCH ?
                       AND f.book = ?
                     ORDER BY f.rank
                     LIMIT ?",
                )
                .bind(&query)
                .bind(b)
                .bind(limit)
                .fetch_all(&self.pool)
                .await
            }
            _ => {
                sqlx::query_as::<_, FtsResult>(
                    "SELECT f.book, f.chapter, f.verse_number, f.text, f.rank
                     FROM verses_fts f
                     JOIN verses v ON v.id = f.rowid
                     WHERE verses_fts MATCH ?
                     ORDER BY f.rank
                     LIMIT ?",
                )
                .bind(&query)
                .bind(limit)
                .fetch_all(&self.pool)
                .await
            }
        };

        results.map_err(|e| DatabaseError::QueryFailed {
            reason: e.to_string(),
        })
    }
}

// ─── FTS query builder ────────────────────────────────────────────────────────

/// Common English stop words (plus KJV-specific function words).
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
    "being",
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
    "their",
    "our",
    "your",
    "my",
    "who",
    "which",
    "what",
    "that",
    "this",
    "these",
    "those",
    "there",
    "here",
    "so",
    "yet",
    // KJV-specific
    "thy",
    "thee",
    "thou",
    "thine",
    "ye",
    "hath",
    "hast",
    "doth",
    "shalt",
    "wilt",
    "wouldest",
    "saith",
    "unto",
    "upon",
    "therein",
    "thereof",
    "wherein",
    "whereby",
    "therefore",
    "wherefore",
];

/// Extract the most significant words from `text` and format them as an
/// FTS5 OR query: `"word1" OR "word2" OR ...`
///
/// Words are lowercased, punctuation stripped, stop words removed, and the
/// top 10 longest (most distinctive) words are selected.
fn build_fts_query(text: &str) -> String {
    let words: Vec<String> = text
        .to_lowercase()
        .split(|c: char| !c.is_alphabetic())
        .filter(|w| w.len() >= 3 && !STOP_WORDS.contains(w))
        .map(|w| format!("\"{}\"", w))
        .collect();

    if words.is_empty() {
        return String::new();
    }

    // Take the 10 longest words — longer words are more distinctive.
    let mut sorted = words;
    sorted.sort_by_key(|b| std::cmp::Reverse(b.len()));
    sorted.truncate(10);
    sorted.join(" OR ")
}
