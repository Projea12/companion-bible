use companion_errors::DatabaseError;

use crate::{models::Church, DbPool};

pub struct ChurchRepository {
    pool: DbPool,
}

impl ChurchRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    /// Return the existing church record, or create one with the supplied data
    /// if the database has no church yet (first-launch path).
    pub async fn get_or_create(
        &self,
        id: &str,
        name: &str,
        region: &str,
    ) -> Result<Church, DatabaseError> {
        if let Some(church) = sqlx::query_as::<_, Church>("SELECT * FROM churches LIMIT 1")
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed {
                reason: e.to_string(),
            })?
        {
            return Ok(church);
        }

        sqlx::query(
            "INSERT INTO churches (id, name, region) VALUES (?, ?, ?)",
        )
        .bind(id)
        .bind(name)
        .bind(region)
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryFailed {
            reason: e.to_string(),
        })?;

        sqlx::query_as("SELECT * FROM churches WHERE id = ?")
            .bind(id)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed {
                reason: e.to_string(),
            })
    }

    /// Upsert a key/value setting for the single church on this device.
    pub async fn update_setting(&self, key: &str, value: &str) -> Result<(), DatabaseError> {
        let church_id = self.church_id().await?;
        sqlx::query(
            "INSERT INTO church_settings (church_id, key, value) VALUES (?, ?, ?)
             ON CONFLICT (church_id, key) DO UPDATE SET value = excluded.value",
        )
        .bind(&church_id)
        .bind(key)
        .bind(value)
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryFailed {
            reason: e.to_string(),
        })?;

        Ok(())
    }

    /// Return the stored value for `key`, or `None` if the setting does not exist.
    pub async fn get_setting(&self, key: &str) -> Result<Option<String>, DatabaseError> {
        let church_id = self.church_id().await?;
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT value FROM church_settings WHERE church_id = ? AND key = ?",
        )
        .bind(&church_id)
        .bind(key)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryFailed {
            reason: e.to_string(),
        })?;

        Ok(row.map(|(v,)| v))
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
            reason: "no church record found — call get_or_create first".into(),
        })
    }
}
