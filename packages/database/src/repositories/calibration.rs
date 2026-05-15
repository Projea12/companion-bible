use companion_errors::DatabaseError;

use crate::{
    models::{CalibrationThresholds, ServiceRecord},
    DbPool,
};

pub struct CalibrationRepository {
    pool: DbPool,
}

impl CalibrationRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    /// Return all calibration threshold rows for this device's church,
    /// one per stage, ordered by stage name.
    pub async fn get_thresholds(&self) -> Result<Vec<CalibrationThresholds>, DatabaseError> {
        let church_id = self.church_id().await?;
        sqlx::query_as(
            "SELECT * FROM calibration_thresholds
             WHERE church_id = ?
             ORDER BY stage ASC",
        )
        .bind(&church_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryFailed {
            reason: e.to_string(),
        })
    }

    /// Upsert a threshold row.  If a row for `(church_id, stage)` already
    /// exists the accept/escalate values are updated in place.
    pub async fn update_thresholds(
        &self,
        thresholds: CalibrationThresholds,
    ) -> Result<(), DatabaseError> {
        sqlx::query(
            "INSERT INTO calibration_thresholds
                 (id, church_id, stage, accept_above, escalate_below, updated_at)
             VALUES (?, ?, ?, ?, ?, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
             ON CONFLICT (church_id, stage) DO UPDATE SET
                 accept_above   = excluded.accept_above,
                 escalate_below = excluded.escalate_below,
                 updated_at     = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')",
        )
        .bind(&thresholds.id)
        .bind(&thresholds.church_id)
        .bind(&thresholds.stage)
        .bind(thresholds.accept_above)
        .bind(thresholds.escalate_below)
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryFailed {
            reason: e.to_string(),
        })?;

        Ok(())
    }

    /// Persist a new service record for a completed sermon.
    pub async fn add_service_record(
        &self,
        record: ServiceRecord,
    ) -> Result<(), DatabaseError> {
        sqlx::query(
            "INSERT INTO service_records
                 (id, sermon_id, total_detections, auto_accepted, operator_corrected,
                  rejected, avg_confidence, avg_processing_time_ms, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&record.id)
        .bind(&record.sermon_id)
        .bind(record.total_detections)
        .bind(record.auto_accepted)
        .bind(record.operator_corrected)
        .bind(record.rejected)
        .bind(record.avg_confidence)
        .bind(record.avg_processing_time_ms)
        .bind(&record.created_at)
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryFailed {
            reason: e.to_string(),
        })?;

        Ok(())
    }

    /// Return the `limit` most recent service records, newest first.
    pub async fn get_recent_service_records(
        &self,
        limit: i64,
    ) -> Result<Vec<ServiceRecord>, DatabaseError> {
        sqlx::query_as(
            "SELECT * FROM service_records ORDER BY created_at DESC LIMIT ?",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryFailed {
            reason: e.to_string(),
        })
    }

    // ── private ───────────────────────────────────────────────────────────────

    async fn church_id(&self) -> Result<String, DatabaseError> {
        let row: Option<(String,)> = sqlx::query_as("SELECT id FROM churches LIMIT 1")
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed {
                reason: e.to_string(),
            })?;

        row.map(|(id,)| id).ok_or_else(|| DatabaseError::QueryFailed {
            reason: "no church record found — call ChurchRepository::get_or_create first".into(),
        })
    }
}
