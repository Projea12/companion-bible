use std::path::PathBuf;
use tempfile::tempdir;

use crate::{close, connect, DbPool, PoolConfig};

// ── helpers ───────────────────────────────────────────────────────────────────

async fn open_db(dir: &std::path::Path) -> DbPool {
    connect(&dir.join("test.db"), &PoolConfig::default())
        .await
        .expect("failed to open test database")
}

fn table_exists_sql(table: &str) -> String {
    format!(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='{table}'"
    )
}

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

// ── schema: all tables exist after migration ──────────────────────────────────

#[tokio::test]
async fn migration_creates_churches_table() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;
    let (count,): (i64,) = sqlx::query_as(&table_exists_sql("churches"))
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1, "churches table should exist");
    close(pool).await;
}

#[tokio::test]
async fn migration_creates_verses_table() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;
    let (count,): (i64,) = sqlx::query_as(&table_exists_sql("verses"))
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1, "verses table should exist");
    close(pool).await;
}

#[tokio::test]
async fn migration_creates_sermons_table() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;
    let (count,): (i64,) = sqlx::query_as(&table_exists_sql("sermons"))
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1, "sermons table should exist");
    close(pool).await;
}

#[tokio::test]
async fn migration_creates_sub_points_table() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;
    let (count,): (i64,) = sqlx::query_as(&table_exists_sql("sub_points"))
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1, "sub_points table should exist");
    close(pool).await;
}

// ── schema: insert / read ─────────────────────────────────────────────────────

#[tokio::test]
async fn can_insert_and_read_church() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;

    sqlx::query(
        "INSERT INTO churches (id, name, region) VALUES ('c1', 'Grace Church', 'Lagos')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let (name,): (String,) =
        sqlx::query_as("SELECT name FROM churches WHERE id = 'c1'")
            .fetch_one(&pool)
            .await
            .unwrap();

    assert_eq!(name, "Grace Church");
    close(pool).await;
}

#[tokio::test]
async fn church_onboarding_defaults_to_false() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;

    sqlx::query(
        "INSERT INTO churches (id, name, region) VALUES ('c1', 'Grace Church', 'Lagos')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let (complete,): (i64,) =
        sqlx::query_as("SELECT onboarding_complete FROM churches WHERE id = 'c1'")
            .fetch_one(&pool)
            .await
            .unwrap();

    assert_eq!(complete, 0, "onboarding_complete should default to 0");
    close(pool).await;
}

#[tokio::test]
async fn can_insert_and_read_sermon() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;

    sqlx::query(
        "INSERT INTO churches (id, name, region) VALUES ('c1', 'Grace Church', 'Lagos')",
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO sermons (id, church_id, pastor, date, started_at)
         VALUES ('s1', 'c1', 'Pastor John', '2026-05-15', '2026-05-15T09:00:00Z')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let (pastor,): (String,) =
        sqlx::query_as("SELECT pastor FROM sermons WHERE id = 's1'")
            .fetch_one(&pool)
            .await
            .unwrap();

    assert_eq!(pastor, "Pastor John");
    close(pool).await;
}

#[tokio::test]
async fn can_insert_and_read_sub_point() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;

    sqlx::query(
        "INSERT INTO churches (id, name, region) VALUES ('c1', 'Grace Church', 'Lagos')",
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO sermons (id, church_id, date, started_at)
         VALUES ('s1', 'c1', '2026-05-15', '2026-05-15T09:00:00Z')",
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO sub_points (id, sermon_id, title, order_index)
         VALUES ('sp1', 's1', 'The Power of Faith', 1)",
    )
    .execute(&pool)
    .await
    .unwrap();

    let (title,): (String,) =
        sqlx::query_as("SELECT title FROM sub_points WHERE id = 'sp1'")
            .fetch_one(&pool)
            .await
            .unwrap();

    assert_eq!(title, "The Power of Faith");
    close(pool).await;
}

#[tokio::test]
async fn can_insert_and_read_verse() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;

    sqlx::query(
        "INSERT INTO verses (book, chapter, verse_number, text, book_order)
         VALUES ('John', 3, 16, 'For God so loved the world...', 43)",
    )
    .execute(&pool)
    .await
    .unwrap();

    let (text,): (String,) =
        sqlx::query_as("SELECT text FROM verses WHERE book='John' AND chapter=3 AND verse_number=16")
            .fetch_one(&pool)
            .await
            .unwrap();

    assert!(text.contains("God so loved"));
    close(pool).await;
}

// ── schema: constraints ───────────────────────────────────────────────────────

#[tokio::test]
async fn sermon_rejects_unknown_church_id() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;

    let result = sqlx::query(
        "INSERT INTO sermons (id, church_id, date, started_at)
         VALUES ('s1', 'no-such-church', '2026-05-15', '2026-05-15T09:00:00Z')",
    )
    .execute(&pool)
    .await;

    assert!(result.is_err(), "FK constraint should reject unknown church_id");
    close(pool).await;
}

#[tokio::test]
async fn sub_point_rejects_unknown_sermon_id() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;

    let result = sqlx::query(
        "INSERT INTO sub_points (id, sermon_id, title, order_index)
         VALUES ('sp1', 'no-such-sermon', 'Point 1', 1)",
    )
    .execute(&pool)
    .await;

    assert!(result.is_err(), "FK constraint should reject unknown sermon_id");
    close(pool).await;
}

#[tokio::test]
async fn verses_unique_constraint_rejects_duplicate() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;

    sqlx::query(
        "INSERT INTO verses (book, chapter, verse_number, text, book_order)
         VALUES ('Genesis', 1, 1, 'In the beginning...', 1)",
    )
    .execute(&pool)
    .await
    .unwrap();

    let result = sqlx::query(
        "INSERT INTO verses (book, chapter, verse_number, text, book_order)
         VALUES ('Genesis', 1, 1, 'Duplicate verse', 1)",
    )
    .execute(&pool)
    .await;

    assert!(result.is_err(), "UNIQUE constraint should reject duplicate verse");
    close(pool).await;
}

#[tokio::test]
async fn deleting_church_cascades_to_sermons() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;

    sqlx::query(
        "INSERT INTO churches (id, name, region) VALUES ('c1', 'Grace Church', 'Lagos')",
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO sermons (id, church_id, date, started_at)
         VALUES ('s1', 'c1', '2026-05-15', '2026-05-15T09:00:00Z')",
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query("DELETE FROM churches WHERE id = 'c1'")
        .execute(&pool)
        .await
        .unwrap();

    let (count,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM sermons WHERE id = 's1'")
            .fetch_one(&pool)
            .await
            .unwrap();

    assert_eq!(count, 0, "deleting a church should cascade-delete its sermons");
    close(pool).await;
}

#[tokio::test]
async fn deleting_sermon_cascades_to_sub_points() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;

    sqlx::query(
        "INSERT INTO churches (id, name, region) VALUES ('c1', 'Grace Church', 'Lagos')",
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO sermons (id, church_id, date, started_at)
         VALUES ('s1', 'c1', '2026-05-15', '2026-05-15T09:00:00Z')",
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO sub_points (id, sermon_id, title, order_index)
         VALUES ('sp1', 's1', 'Point 1', 1)",
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query("DELETE FROM sermons WHERE id = 's1'")
        .execute(&pool)
        .await
        .unwrap();

    let (count,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM sub_points WHERE id = 'sp1'")
            .fetch_one(&pool)
            .await
            .unwrap();

    assert_eq!(count, 0, "deleting a sermon should cascade-delete its sub_points");
    close(pool).await;
}
