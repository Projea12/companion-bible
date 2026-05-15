use std::path::PathBuf;
use tempfile::tempdir;

use crate::{close, connect, PoolConfig};

fn test_db_path(dir: &std::path::Path) -> PathBuf {
    dir.join("test.db")
}

// ── connect & initialize ──────────────────────────────────────────────────────

#[tokio::test]
async fn connect_creates_database_file() {
    let dir = tempdir().unwrap();
    let path = test_db_path(dir.path());

    assert!(!path.exists(), "db should not exist before connect");
    let pool = connect(&path, &PoolConfig::default()).await.unwrap();
    assert!(path.exists(), "db file should be created after connect");
    close(pool).await;
}

#[tokio::test]
async fn connect_twice_reuses_existing_file() {
    let dir = tempdir().unwrap();
    let path = test_db_path(dir.path());

    let pool1 = connect(&path, &PoolConfig::default()).await.unwrap();
    close(pool1).await;

    // Second connect must succeed and not corrupt the existing file.
    let pool2 = connect(&path, &PoolConfig::default()).await.unwrap();
    close(pool2).await;
}

#[tokio::test]
async fn connect_creates_parent_directories() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("nested").join("dirs").join("test.db");

    let pool = connect(&path, &PoolConfig::default()).await.unwrap();
    assert!(path.exists());
    close(pool).await;
}

// ── pool is functional after connect ─────────────────────────────────────────

#[tokio::test]
async fn pool_can_execute_simple_query() {
    let dir = tempdir().unwrap();
    let pool = connect(&test_db_path(dir.path()), &PoolConfig::default())
        .await
        .unwrap();

    let row: (i64,) = sqlx::query_as("SELECT 1").fetch_one(&pool).await.unwrap();
    assert_eq!(row.0, 1);
    close(pool).await;
}

#[tokio::test]
async fn wal_mode_is_enabled() {
    let dir = tempdir().unwrap();
    let pool = connect(&test_db_path(dir.path()), &PoolConfig::default())
        .await
        .unwrap();

    let row: (String,) =
        sqlx::query_as("PRAGMA journal_mode").fetch_one(&pool).await.unwrap();
    assert_eq!(row.0, "wal", "journal_mode should be WAL");
    close(pool).await;
}

#[tokio::test]
async fn foreign_keys_are_enabled() {
    let dir = tempdir().unwrap();
    let pool = connect(&test_db_path(dir.path()), &PoolConfig::default())
        .await
        .unwrap();

    let row: (i64,) =
        sqlx::query_as("PRAGMA foreign_keys").fetch_one(&pool).await.unwrap();
    assert_eq!(row.0, 1, "foreign_keys should be ON");
    close(pool).await;
}

// ── pool config ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn default_pool_config_has_sensible_values() {
    let config = PoolConfig::default();
    assert_eq!(config.max_connections, 5);
    assert!(config.connect_timeout.as_secs() > 0);
    assert!(config.idle_timeout.as_secs() > 0);
}

#[tokio::test]
async fn custom_pool_config_is_applied() {
    use std::time::Duration;

    let dir = tempdir().unwrap();
    let config = PoolConfig {
        max_connections: 2,
        connect_timeout: Duration::from_secs(10),
        idle_timeout: Duration::from_secs(60),
    };
    let pool = connect(&test_db_path(dir.path()), &config).await.unwrap();
    close(pool).await;
}

// ── graceful close ────────────────────────────────────────────────────────────

#[tokio::test]
async fn close_shuts_down_pool() {
    let dir = tempdir().unwrap();
    let pool = connect(&test_db_path(dir.path()), &PoolConfig::default())
        .await
        .unwrap();

    let pool_clone = pool.clone();
    close(pool).await;

    // After close, the pool is shut down and new queries should fail.
    assert!(pool_clone.is_closed(), "pool should be closed after close()");
}

// ── bad path ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn connect_fails_gracefully_on_invalid_path() {
    // A path whose parent is a file (not a directory) cannot be created.
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("file.txt");
    std::fs::write(&file_path, b"hello").unwrap();
    let db_path = file_path.join("impossible.db");

    let result = connect(&db_path, &PoolConfig::default()).await;
    assert!(result.is_err(), "should fail when parent path is a file");
}
