use std::{path::Path, time::Duration};

use companion_errors::DatabaseError;
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
    SqlitePool,
};

/// Shared connection pool — cheap to clone, safe to share across threads.
pub type DbPool = SqlitePool;

/// Tuning knobs for the connection pool.
pub struct PoolConfig {
    /// Maximum number of simultaneous SQLite connections.
    /// SQLite benefits little from >5 since writes are serialised anyway.
    pub max_connections: u32,
    /// How long to wait for a connection from the pool before giving up.
    pub connect_timeout: Duration,
    /// How long an idle connection stays open before being closed.
    pub idle_timeout: Duration,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            max_connections: 5,
            connect_timeout: Duration::from_secs(30),
            idle_timeout: Duration::from_secs(600),
        }
    }
}

/// Open (or create) the SQLite database at `db_path`, apply any pending
/// migrations, and return a ready-to-use connection pool.
///
/// On first launch the file is created automatically.  WAL journal mode and
/// foreign-key enforcement are enabled for every connection.
pub async fn connect(db_path: &Path, config: &PoolConfig) -> Result<DbPool, DatabaseError> {
    // Ensure the parent directory exists.
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| DatabaseError::QueryFailed {
            reason: format!("could not create database directory: {e}"),
        })?;
    }

    let connect_options = SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true)
        // Write-ahead log: readers don't block writers and vice versa.
        .journal_mode(SqliteJournalMode::Wal)
        // Enforce FOREIGN KEY constraints on every connection.
        .foreign_keys(true)
        // NORMAL sync is safe with WAL and gives a good speed/safety tradeoff.
        .synchronous(SqliteSynchronous::Normal)
        // Wait up to 5 s when another process/thread holds the write lock.
        .busy_timeout(Duration::from_secs(5));

    let pool = SqlitePoolOptions::new()
        .max_connections(config.max_connections)
        .acquire_timeout(config.connect_timeout)
        .idle_timeout(config.idle_timeout)
        .connect_with(connect_options)
        .await
        .map_err(|e| map_sqlx_error(e, db_path))?;

    run_migrations(&pool).await?;

    Ok(pool)
}

/// Wait for all in-flight queries to finish and close every connection.
///
/// Call this during application shutdown before the process exits.
pub async fn close(pool: DbPool) {
    pool.close().await;
}

// ─── Migration runner ─────────────────────────────────────────────────────────

async fn run_migrations(pool: &DbPool) -> Result<(), DatabaseError> {
    sqlx::migrate!()
        .run(pool)
        .await
        .map_err(|e| DatabaseError::MigrationFailed {
            from: 0,
            to: 0,
            reason: e.to_string(),
        })
}

// ─── Error mapping ────────────────────────────────────────────────────────────

fn map_sqlx_error(e: sqlx::Error, db_path: &Path) -> DatabaseError {
    match &e {
        sqlx::Error::Io(io) if io.kind() == std::io::ErrorKind::NotFound => {
            DatabaseError::FileNotFound {
                path: db_path.display().to_string(),
            }
        }
        sqlx::Error::Database(db_err) => {
            // SQLite error code 5 = SQLITE_BUSY, 6 = SQLITE_LOCKED
            if db_err.code().map_or(false, |c| c == "5" || c == "6") {
                DatabaseError::Locked
            } else {
                DatabaseError::QueryFailed {
                    reason: db_err.message().to_string(),
                }
            }
        }
        _ => DatabaseError::QueryFailed {
            reason: e.to_string(),
        },
    }
}
