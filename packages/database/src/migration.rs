use companion_errors::DatabaseError;

use crate::connection::DbPool;

// ─── Public types ─────────────────────────────────────────────────────────────

/// A migration that has already been applied to the database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedMigration {
    /// Sequential version number (matches the numeric prefix in the file name).
    pub version: i64,
    /// Human-readable description (the part after the number in the file name).
    pub description: String,
    /// ISO 8601 timestamp of when this migration was applied.
    pub installed_on: String,
    /// How long the migration took to run, in milliseconds.
    pub execution_time_ms: i64,
}

// ─── Runner ───────────────────────────────────────────────────────────────────

/// Apply all pending migrations to `pool`.
///
/// This is idempotent — already-applied migrations are skipped.
/// `connect()` calls this automatically; expose it here for callers that need
/// to trigger migrations independently (e.g. in a maintenance tool).
pub async fn run(pool: &DbPool) -> Result<(), DatabaseError> {
    sqlx::migrate!()
        .run(pool)
        .await
        .map_err(|e| DatabaseError::MigrationFailed {
            from: 0,
            to: 0,
            reason: e.to_string(),
        })
}

// ─── Version tracking ─────────────────────────────────────────────────────────

/// Return the version number of the most recently applied migration.
///
/// Returns `0` when no migrations have been applied yet (empty database).
pub async fn current_version(pool: &DbPool) -> Result<i64, DatabaseError> {
    let row: Option<(i64,)> =
        sqlx::query_as("SELECT MAX(version) FROM _sqlx_migrations WHERE success = TRUE")
            .fetch_optional(pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed {
                reason: e.to_string(),
            })?;

    Ok(row.map(|(v,)| v).unwrap_or(0))
}

/// Return `true` when all embedded migrations have been applied successfully.
pub async fn is_up_to_date(pool: &DbPool) -> Result<bool, DatabaseError> {
    let expected = sqlx::migrate!().migrations.len() as i64;

    let (applied,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM _sqlx_migrations WHERE success = TRUE")
            .fetch_one(pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed {
                reason: e.to_string(),
            })?;

    Ok(applied >= expected)
}

/// Return every migration that has been successfully applied, ordered by
/// version ascending.
pub async fn list_applied(pool: &DbPool) -> Result<Vec<AppliedMigration>, DatabaseError> {
    let rows: Vec<(i64, String, String, i64)> = sqlx::query_as(
        "SELECT version, description, installed_on, execution_time
         FROM _sqlx_migrations
         WHERE success = TRUE
         ORDER BY version ASC",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| DatabaseError::QueryFailed {
        reason: e.to_string(),
    })?;

    Ok(rows
        .into_iter()
        .map(
            |(version, description, installed_on, execution_time_ns)| AppliedMigration {
                version,
                description,
                installed_on,
                // sqlx stores execution_time in nanoseconds; convert to ms.
                execution_time_ms: execution_time_ns / 1_000_000,
            },
        )
        .collect())
}
