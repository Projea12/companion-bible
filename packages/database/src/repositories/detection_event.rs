use companion_errors::DatabaseError;

use crate::{models::DetectionEvent, DbPool};

pub struct DetectionEventRepository {
    pool: DbPool,
}

impl DetectionEventRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, event: DetectionEvent) -> Result<DetectionEvent, DatabaseError> {
        sqlx::query(
            "INSERT INTO detection_events
                 (id, sermon_id, raw_transcript, pattern_result, local_ai_result,
                  cloud_ai_result, final_reference, confidence, decision,
                  operator_action, correct_reference, processing_time_ms, timestamp)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&event.id)
        .bind(&event.sermon_id)
        .bind(&event.raw_transcript)
        .bind(&event.pattern_result)
        .bind(&event.local_ai_result)
        .bind(&event.cloud_ai_result)
        .bind(&event.final_reference)
        .bind(event.confidence)
        .bind(&event.decision)
        .bind(&event.operator_action)
        .bind(&event.correct_reference)
        .bind(event.processing_time_ms)
        .bind(&event.timestamp)
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryFailed {
            reason: e.to_string(),
        })?;

        Ok(event)
    }

    pub async fn get_for_sermon(
        &self,
        sermon_id: &str,
    ) -> Result<Vec<DetectionEvent>, DatabaseError> {
        sqlx::query_as(
            "SELECT * FROM detection_events WHERE sermon_id = ? ORDER BY timestamp ASC",
        )
        .bind(sermon_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryFailed {
            reason: e.to_string(),
        })
    }

    pub async fn update_operator_action(
        &self,
        id: &str,
        action: &str,
        correct_reference: Option<&str>,
    ) -> Result<(), DatabaseError> {
        sqlx::query(
            "UPDATE detection_events
             SET operator_action = ?, correct_reference = ?
             WHERE id = ?",
        )
        .bind(action)
        .bind(correct_reference)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryFailed {
            reason: e.to_string(),
        })?;

        Ok(())
    }
}
