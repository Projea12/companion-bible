use companion_errors::DatabaseError;

use crate::{models::Verse, DbPool};

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
        sqlx::query_as(
            "SELECT * FROM verses WHERE book = ? AND chapter = ? AND verse_number = ?",
        )
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
        sqlx::query_as(
            "SELECT * FROM verses WHERE book = ? ORDER BY chapter, verse_number",
        )
        .bind(book)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryFailed {
            reason: e.to_string(),
        })
    }
}
