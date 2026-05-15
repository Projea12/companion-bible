use companion_errors::DatabaseError;

use crate::{models::Sermon, DbPool};

pub struct SermonRepository {
    pool: DbPool,
}

impl SermonRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, sermon: Sermon) -> Result<Sermon, DatabaseError> {
        sqlx::query(
            "INSERT INTO sermons
                 (id, church_id, title, pastor, date, anchor_scripture, started_at, ended_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&sermon.id)
        .bind(&sermon.church_id)
        .bind(&sermon.title)
        .bind(&sermon.pastor)
        .bind(&sermon.date)
        .bind(&sermon.anchor_scripture)
        .bind(&sermon.started_at)
        .bind(&sermon.ended_at)
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryFailed {
            reason: e.to_string(),
        })?;

        Ok(sermon)
    }

    pub async fn update(&self, sermon: Sermon) -> Result<Sermon, DatabaseError> {
        sqlx::query(
            "UPDATE sermons
             SET title = ?, pastor = ?, date = ?, anchor_scripture = ?, ended_at = ?
             WHERE id = ?",
        )
        .bind(&sermon.title)
        .bind(&sermon.pastor)
        .bind(&sermon.date)
        .bind(&sermon.anchor_scripture)
        .bind(&sermon.ended_at)
        .bind(&sermon.id)
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryFailed {
            reason: e.to_string(),
        })?;

        Ok(sermon)
    }

    pub async fn get_by_id(&self, id: &str) -> Result<Option<Sermon>, DatabaseError> {
        sqlx::query_as("SELECT * FROM sermons WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed {
                reason: e.to_string(),
            })
    }

    pub async fn get_recent(&self, limit: i64) -> Result<Vec<Sermon>, DatabaseError> {
        sqlx::query_as("SELECT * FROM sermons ORDER BY started_at DESC LIMIT ?")
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed {
                reason: e.to_string(),
            })
    }

    pub async fn end_sermon(&self, id: &str, ended_at: &str) -> Result<(), DatabaseError> {
        sqlx::query("UPDATE sermons SET ended_at = ? WHERE id = ?")
            .bind(ended_at)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed {
                reason: e.to_string(),
            })?;

        Ok(())
    }
}
