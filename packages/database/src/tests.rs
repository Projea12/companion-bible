use std::path::PathBuf;
use tempfile::tempdir;

use crate::{
    close, connect, migration, CalibrationThresholds, Church, ChurchSettings, DetectionEvent,
    DbPool, PoolConfig, Sermon, ServiceRecord, SubPoint, Verse,
};

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

// ── schema: migration 002 — tables exist ─────────────────────────────────────

#[tokio::test]
async fn migration_creates_detection_events_table() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;
    let (count,): (i64,) = sqlx::query_as(&table_exists_sql("detection_events"))
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1, "detection_events table should exist");
    close(pool).await;
}

#[tokio::test]
async fn migration_creates_app_state_table() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;
    let (count,): (i64,) = sqlx::query_as(&table_exists_sql("app_state"))
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1, "app_state table should exist");
    close(pool).await;
}

#[tokio::test]
async fn migration_creates_church_settings_table() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;
    let (count,): (i64,) = sqlx::query_as(&table_exists_sql("church_settings"))
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1, "church_settings table should exist");
    close(pool).await;
}

// ── schema: detection_events ──────────────────────────────────────────────────

#[tokio::test]
async fn can_insert_and_read_detection_event() {
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
        "INSERT INTO detection_events
             (id, sermon_id, raw_transcript, decision, confidence, processing_time_ms)
         VALUES ('d1', 's1', 'John 3:16', 'pattern', 0.98, 42)",
    )
    .execute(&pool)
    .await
    .unwrap();

    let (transcript,): (String,) =
        sqlx::query_as("SELECT raw_transcript FROM detection_events WHERE id = 'd1'")
            .fetch_one(&pool)
            .await
            .unwrap();

    assert_eq!(transcript, "John 3:16");
    close(pool).await;
}

#[tokio::test]
async fn detection_event_rejects_invalid_sermon_id() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;

    let result = sqlx::query(
        "INSERT INTO detection_events
             (id, sermon_id, raw_transcript, decision, confidence, processing_time_ms)
         VALUES ('d1', 'no-such-sermon', 'John 3:16', 'pattern', 0.9, 10)",
    )
    .execute(&pool)
    .await;

    assert!(result.is_err(), "FK constraint should reject unknown sermon_id");
    close(pool).await;
}

#[tokio::test]
async fn detection_event_rejects_confidence_out_of_range() {
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

    let result = sqlx::query(
        "INSERT INTO detection_events
             (id, sermon_id, raw_transcript, decision, confidence, processing_time_ms)
         VALUES ('d1', 's1', 'test', 'pattern', 1.5, 10)",
    )
    .execute(&pool)
    .await;

    assert!(result.is_err(), "CHECK constraint should reject confidence > 1.0");
    close(pool).await;
}

#[tokio::test]
async fn deleting_sermon_cascades_to_detection_events() {
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
        "INSERT INTO detection_events
             (id, sermon_id, raw_transcript, decision, confidence, processing_time_ms)
         VALUES ('d1', 's1', 'John 3:16', 'pattern', 0.9, 10)",
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query("DELETE FROM sermons WHERE id = 's1'")
        .execute(&pool)
        .await
        .unwrap();

    let (count,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM detection_events WHERE id = 'd1'")
            .fetch_one(&pool)
            .await
            .unwrap();

    assert_eq!(count, 0, "deleting sermon should cascade-delete detection_events");
    close(pool).await;
}

// ── schema: app_state ─────────────────────────────────────────────────────────

#[tokio::test]
async fn can_insert_and_read_app_state() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;

    sqlx::query(
        "INSERT INTO app_state (key, value) VALUES ('display_mode', '\"fullscreen\"')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let (value,): (String,) =
        sqlx::query_as("SELECT value FROM app_state WHERE key = 'display_mode'")
            .fetch_one(&pool)
            .await
            .unwrap();

    assert_eq!(value, "\"fullscreen\"");
    close(pool).await;
}

#[tokio::test]
async fn app_state_rejects_duplicate_key() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;

    sqlx::query("INSERT INTO app_state (key, value) VALUES ('theme', '\"dark\"')")
        .execute(&pool)
        .await
        .unwrap();

    let result =
        sqlx::query("INSERT INTO app_state (key, value) VALUES ('theme', '\"light\"')")
            .execute(&pool)
            .await;

    assert!(result.is_err(), "PRIMARY KEY should reject duplicate key");
    close(pool).await;
}

// ── schema: church_settings ───────────────────────────────────────────────────

#[tokio::test]
async fn can_insert_and_read_church_setting() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;

    sqlx::query(
        "INSERT INTO churches (id, name, region) VALUES ('c1', 'Grace Church', 'Lagos')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO church_settings (church_id, key, value)
         VALUES ('c1', 'preferred_translation', '\"KJV\"')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let (value,): (String,) = sqlx::query_as(
        "SELECT value FROM church_settings WHERE church_id = 'c1' AND key = 'preferred_translation'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(value, "\"KJV\"");
    close(pool).await;
}

#[tokio::test]
async fn church_settings_rejects_duplicate_key_for_same_church() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;

    sqlx::query(
        "INSERT INTO churches (id, name, region) VALUES ('c1', 'Grace Church', 'Lagos')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO church_settings (church_id, key, value) VALUES ('c1', 'font_size', '16')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let result = sqlx::query(
        "INSERT INTO church_settings (church_id, key, value) VALUES ('c1', 'font_size', '18')",
    )
    .execute(&pool)
    .await;

    assert!(result.is_err(), "composite PK should reject duplicate (church_id, key)");
    close(pool).await;
}

#[tokio::test]
async fn church_settings_rejects_unknown_church_id() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;

    let result = sqlx::query(
        "INSERT INTO church_settings (church_id, key, value)
         VALUES ('no-such-church', 'font_size', '16')",
    )
    .execute(&pool)
    .await;

    assert!(result.is_err(), "FK should reject unknown church_id");
    close(pool).await;
}

#[tokio::test]
async fn deleting_church_cascades_to_church_settings() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;

    sqlx::query(
        "INSERT INTO churches (id, name, region) VALUES ('c1', 'Grace Church', 'Lagos')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO church_settings (church_id, key, value) VALUES ('c1', 'font_size', '16')",
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query("DELETE FROM churches WHERE id = 'c1'")
        .execute(&pool)
        .await
        .unwrap();

    let (count,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM church_settings WHERE church_id = 'c1'")
            .fetch_one(&pool)
            .await
            .unwrap();

    assert_eq!(count, 0, "deleting church should cascade-delete church_settings");
    close(pool).await;
}

// ── migration 003: calibration_thresholds ─────────────────────────────────────

#[tokio::test]
async fn migration_creates_calibration_thresholds_table() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;
    let (count,): (i64,) = sqlx::query_as(&table_exists_sql("calibration_thresholds"))
        .fetch_one(&pool).await.unwrap();
    assert_eq!(count, 1);
    close(pool).await;
}

#[tokio::test]
async fn can_insert_and_read_calibration_threshold() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;

    sqlx::query("INSERT INTO churches (id, name, region) VALUES ('c1', 'Grace', 'Lagos')")
        .execute(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO calibration_thresholds (id, church_id, stage, accept_above, escalate_below)
         VALUES ('t1', 'c1', 'pattern', 0.85, 0.4)",
    )
    .execute(&pool).await.unwrap();

    let (stage,): (String,) =
        sqlx::query_as("SELECT stage FROM calibration_thresholds WHERE id = 't1'")
            .fetch_one(&pool).await.unwrap();
    assert_eq!(stage, "pattern");
    close(pool).await;
}

#[tokio::test]
async fn calibration_threshold_rejects_invalid_stage() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;

    sqlx::query("INSERT INTO churches (id, name, region) VALUES ('c1', 'Grace', 'Lagos')")
        .execute(&pool).await.unwrap();
    let result = sqlx::query(
        "INSERT INTO calibration_thresholds (id, church_id, stage, accept_above, escalate_below)
         VALUES ('t1', 'c1', 'unknown_stage', 0.9, 0.5)",
    )
    .execute(&pool).await;
    assert!(result.is_err(), "CHECK should reject unknown stage");
    close(pool).await;
}

#[tokio::test]
async fn calibration_threshold_rejects_inverted_thresholds() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;

    sqlx::query("INSERT INTO churches (id, name, region) VALUES ('c1', 'Grace', 'Lagos')")
        .execute(&pool).await.unwrap();
    let result = sqlx::query(
        "INSERT INTO calibration_thresholds (id, church_id, stage, accept_above, escalate_below)
         VALUES ('t1', 'c1', 'pattern', 0.3, 0.8)",
    )
    .execute(&pool).await;
    assert!(result.is_err(), "CHECK should reject accept_above < escalate_below");
    close(pool).await;
}

#[tokio::test]
async fn calibration_threshold_unique_per_church_stage() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;

    sqlx::query("INSERT INTO churches (id, name, region) VALUES ('c1', 'Grace', 'Lagos')")
        .execute(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO calibration_thresholds (id, church_id, stage) VALUES ('t1', 'c1', 'pattern')",
    )
    .execute(&pool).await.unwrap();
    let result = sqlx::query(
        "INSERT INTO calibration_thresholds (id, church_id, stage) VALUES ('t2', 'c1', 'pattern')",
    )
    .execute(&pool).await;
    assert!(result.is_err(), "UNIQUE should reject duplicate (church_id, stage)");
    close(pool).await;
}

// ── migration 003: service_records ────────────────────────────────────────────

#[tokio::test]
async fn migration_creates_service_records_table() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;
    let (count,): (i64,) = sqlx::query_as(&table_exists_sql("service_records"))
        .fetch_one(&pool).await.unwrap();
    assert_eq!(count, 1);
    close(pool).await;
}

#[tokio::test]
async fn can_insert_and_read_service_record() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;

    sqlx::query("INSERT INTO churches (id, name, region) VALUES ('c1', 'Grace', 'Lagos')")
        .execute(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO sermons (id, church_id, date, started_at)
         VALUES ('s1', 'c1', '2026-05-15', '2026-05-15T09:00:00Z')",
    )
    .execute(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO service_records (id, sermon_id, total_detections, auto_accepted)
         VALUES ('r1', 's1', 10, 8)",
    )
    .execute(&pool).await.unwrap();

    let (total,): (i64,) =
        sqlx::query_as("SELECT total_detections FROM service_records WHERE id = 'r1'")
            .fetch_one(&pool).await.unwrap();
    assert_eq!(total, 10);
    close(pool).await;
}

#[tokio::test]
async fn service_record_unique_per_sermon() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;

    sqlx::query("INSERT INTO churches (id, name, region) VALUES ('c1', 'Grace', 'Lagos')")
        .execute(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO sermons (id, church_id, date, started_at)
         VALUES ('s1', 'c1', '2026-05-15', '2026-05-15T09:00:00Z')",
    )
    .execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO service_records (id, sermon_id) VALUES ('r1', 's1')")
        .execute(&pool).await.unwrap();
    let result =
        sqlx::query("INSERT INTO service_records (id, sermon_id) VALUES ('r2', 's1')")
            .execute(&pool).await;
    assert!(result.is_err(), "UNIQUE should allow only one service_record per sermon");
    close(pool).await;
}

// ── migration 003: operator_patterns ─────────────────────────────────────────

#[tokio::test]
async fn migration_creates_operator_patterns_table() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;
    let (count,): (i64,) = sqlx::query_as(&table_exists_sql("operator_patterns"))
        .fetch_one(&pool).await.unwrap();
    assert_eq!(count, 1);
    close(pool).await;
}

#[tokio::test]
async fn can_insert_and_read_operator_pattern() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;

    sqlx::query("INSERT INTO churches (id, name, region) VALUES ('c1', 'Grace', 'Lagos')")
        .execute(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO operator_patterns (id, church_id, pattern, book_name)
         VALUES ('p1', 'c1', 'Ps.', 'Psalms')",
    )
    .execute(&pool).await.unwrap();

    let (book,): (String,) =
        sqlx::query_as("SELECT book_name FROM operator_patterns WHERE id = 'p1'")
            .fetch_one(&pool).await.unwrap();
    assert_eq!(book, "Psalms");
    close(pool).await;
}

#[tokio::test]
async fn operator_pattern_rejects_invalid_match_type() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;

    sqlx::query("INSERT INTO churches (id, name, region) VALUES ('c1', 'Grace', 'Lagos')")
        .execute(&pool).await.unwrap();
    let result = sqlx::query(
        "INSERT INTO operator_patterns (id, church_id, pattern, book_name, match_type)
         VALUES ('p1', 'c1', 'Ps.', 'Psalms', 'fuzzy')",
    )
    .execute(&pool).await;
    assert!(result.is_err(), "CHECK should reject unknown match_type");
    close(pool).await;
}

// ── migration 004: acoustic_profiles ─────────────────────────────────────────

#[tokio::test]
async fn migration_creates_acoustic_profiles_table() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;
    let (count,): (i64,) = sqlx::query_as(&table_exists_sql("acoustic_profiles"))
        .fetch_one(&pool).await.unwrap();
    assert_eq!(count, 1);
    close(pool).await;
}

#[tokio::test]
async fn can_insert_and_read_acoustic_profile() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;

    sqlx::query("INSERT INTO churches (id, name, region) VALUES ('c1', 'Grace', 'Lagos')")
        .execute(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO acoustic_profiles (id, church_id, name, sample_rate, vad_threshold)
         VALUES ('a1', 'c1', 'Main Hall', 16000, 0.6)",
    )
    .execute(&pool).await.unwrap();

    let (name,): (String,) =
        sqlx::query_as("SELECT name FROM acoustic_profiles WHERE id = 'a1'")
            .fetch_one(&pool).await.unwrap();
    assert_eq!(name, "Main Hall");
    close(pool).await;
}

#[tokio::test]
async fn acoustic_profile_rejects_invalid_vad_threshold() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;

    sqlx::query("INSERT INTO churches (id, name, region) VALUES ('c1', 'Grace', 'Lagos')")
        .execute(&pool).await.unwrap();
    let result = sqlx::query(
        "INSERT INTO acoustic_profiles (id, church_id, name, vad_threshold)
         VALUES ('a1', 'c1', 'Test', 1.5)",
    )
    .execute(&pool).await;
    assert!(result.is_err(), "CHECK should reject vad_threshold > 1.0");
    close(pool).await;
}

// ── migration 004: hardware_profiles ─────────────────────────────────────────

#[tokio::test]
async fn migration_creates_hardware_profiles_table() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;
    let (count,): (i64,) = sqlx::query_as(&table_exists_sql("hardware_profiles"))
        .fetch_one(&pool).await.unwrap();
    assert_eq!(count, 1);
    close(pool).await;
}

#[tokio::test]
async fn can_insert_and_read_hardware_profile() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;

    sqlx::query("INSERT INTO churches (id, name, region) VALUES ('c1', 'Grace', 'Lagos')")
        .execute(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO hardware_profiles (id, church_id, device_name, device_id)
         VALUES ('h1', 'c1', 'USB Microphone', 'dev-001')",
    )
    .execute(&pool).await.unwrap();

    let (name,): (String,) =
        sqlx::query_as("SELECT device_name FROM hardware_profiles WHERE id = 'h1'")
            .fetch_one(&pool).await.unwrap();
    assert_eq!(name, "USB Microphone");
    close(pool).await;
}

#[tokio::test]
async fn hardware_profile_unique_per_church_device() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;

    sqlx::query("INSERT INTO churches (id, name, region) VALUES ('c1', 'Grace', 'Lagos')")
        .execute(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO hardware_profiles (id, church_id, device_name, device_id)
         VALUES ('h1', 'c1', 'USB Mic', 'dev-001')",
    )
    .execute(&pool).await.unwrap();
    let result = sqlx::query(
        "INSERT INTO hardware_profiles (id, church_id, device_name, device_id)
         VALUES ('h2', 'c1', 'USB Mic v2', 'dev-001')",
    )
    .execute(&pool).await;
    assert!(result.is_err(), "UNIQUE should reject duplicate (church_id, device_id)");
    close(pool).await;
}

// ── migration ordering and version tracking ───────────────────────────────────

#[tokio::test]
async fn current_version_returns_4_after_all_migrations() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;
    let version = migration::current_version(&pool).await.unwrap();
    assert_eq!(version, 4, "4 migrations should have been applied");
    close(pool).await;
}

#[tokio::test]
async fn is_up_to_date_returns_true_after_connect() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;
    assert!(migration::is_up_to_date(&pool).await.unwrap());
    close(pool).await;
}

#[tokio::test]
async fn list_applied_returns_4_migrations_in_order() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;
    let applied = migration::list_applied(&pool).await.unwrap();
    assert_eq!(applied.len(), 4, "should have 4 applied migrations");
    for (i, m) in applied.iter().enumerate() {
        assert_eq!(m.version, (i + 1) as i64, "migrations must be sorted by version");
    }
    close(pool).await;
}

#[tokio::test]
async fn migrations_are_idempotent() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;

    // Running migrations a second time must not re-apply any migration.
    migration::run(&pool).await.unwrap();

    let applied = migration::list_applied(&pool).await.unwrap();
    assert_eq!(applied.len(), 4, "second run must not add duplicate entries");
    close(pool).await;
}

#[tokio::test]
async fn migration_versions_are_sequential() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;
    let applied = migration::list_applied(&pool).await.unwrap();
    let versions: Vec<i64> = applied.iter().map(|m| m.version).collect();
    assert_eq!(versions, vec![1, 2, 3, 4], "versions must be 1..4 in order");
    close(pool).await;
}

#[tokio::test]
async fn applied_migrations_have_descriptions() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;
    let applied = migration::list_applied(&pool).await.unwrap();
    for m in &applied {
        assert!(!m.description.is_empty(), "every migration must have a description");
        assert!(!m.installed_on.is_empty(), "every migration must have an installed_on timestamp");
    }
    close(pool).await;
}

// ── models: sqlx::FromRow ─────────────────────────────────────────────────────

#[tokio::test]
async fn church_from_row_maps_all_fields() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;

    sqlx::query(
        "INSERT INTO churches (id, name, region, onboarding_complete)
         VALUES ('c1', 'Grace Church', 'Lagos', 1)",
    )
    .execute(&pool)
    .await
    .unwrap();

    let church: Church = sqlx::query_as("SELECT * FROM churches WHERE id = 'c1'")
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_eq!(church.id, "c1");
    assert_eq!(church.name, "Grace Church");
    assert_eq!(church.region, "Lagos");
    assert!(church.onboarding_complete);
    assert!(!church.installed_at.is_empty());
    close(pool).await;
}

#[tokio::test]
async fn verse_from_row_maps_all_fields() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;

    sqlx::query(
        "INSERT INTO verses (book, chapter, verse_number, text, book_order)
         VALUES ('John', 3, 16, 'For God so loved the world...', 43)",
    )
    .execute(&pool)
    .await
    .unwrap();

    let verse: Verse = sqlx::query_as(
        "SELECT * FROM verses WHERE book='John' AND chapter=3 AND verse_number=16",
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    assert!(verse.id > 0);
    assert_eq!(verse.book, "John");
    assert_eq!(verse.chapter, 3);
    assert_eq!(verse.verse_number, 16);
    assert_eq!(verse.book_order, 43);
    assert!(verse.text.contains("God so loved"));
    close(pool).await;
}

#[tokio::test]
async fn sermon_from_row_maps_optional_fields() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;

    sqlx::query(
        "INSERT INTO churches (id, name, region) VALUES ('c1', 'Grace', 'Lagos')",
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

    let sermon: Sermon = sqlx::query_as("SELECT * FROM sermons WHERE id = 's1'")
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_eq!(sermon.id, "s1");
    assert_eq!(sermon.church_id, "c1");
    assert!(sermon.title.is_none());
    assert!(sermon.pastor.is_none());
    assert!(sermon.ended_at.is_none());
    close(pool).await;
}

#[tokio::test]
async fn sub_point_from_row_maps_all_fields() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;

    sqlx::query(
        "INSERT INTO churches (id, name, region) VALUES ('c1', 'Grace', 'Lagos')",
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
         VALUES ('sp1', 's1', 'Introduction', 1)",
    )
    .execute(&pool)
    .await
    .unwrap();

    let sp: SubPoint = sqlx::query_as("SELECT * FROM sub_points WHERE id = 'sp1'")
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_eq!(sp.id, "sp1");
    assert_eq!(sp.title, "Introduction");
    assert_eq!(sp.order_index, 1);
    assert!(sp.started_at.is_none());
    close(pool).await;
}

#[tokio::test]
async fn detection_event_from_row_maps_all_fields() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;

    sqlx::query(
        "INSERT INTO churches (id, name, region) VALUES ('c1', 'Grace', 'Lagos')",
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
        "INSERT INTO detection_events
             (id, sermon_id, raw_transcript, decision, confidence, processing_time_ms,
              final_reference)
         VALUES ('d1', 's1', 'John 3:16', 'auto_accept', 0.98, 42, 'John 3:16')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let ev: DetectionEvent =
        sqlx::query_as("SELECT * FROM detection_events WHERE id = 'd1'")
            .fetch_one(&pool)
            .await
            .unwrap();

    assert_eq!(ev.id, "d1");
    assert_eq!(ev.raw_transcript, "John 3:16");
    assert!((ev.confidence - 0.98).abs() < f64::EPSILON);
    assert_eq!(ev.processing_time_ms, 42);
    assert_eq!(ev.final_reference.as_deref(), Some("John 3:16"));
    assert!(ev.pattern_result.is_none());
    close(pool).await;
}

#[tokio::test]
async fn church_settings_from_row_maps_all_fields() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;

    sqlx::query(
        "INSERT INTO churches (id, name, region) VALUES ('c1', 'Grace', 'Lagos')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO church_settings (church_id, key, value)
         VALUES ('c1', 'preferred_translation', '\"KJV\"')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let cs: ChurchSettings = sqlx::query_as(
        "SELECT * FROM church_settings WHERE church_id = 'c1' AND key = 'preferred_translation'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(cs.church_id, "c1");
    assert_eq!(cs.key, "preferred_translation");
    assert_eq!(cs.value, "\"KJV\"");
    close(pool).await;
}

#[tokio::test]
async fn calibration_thresholds_from_row_maps_all_fields() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;

    sqlx::query(
        "INSERT INTO churches (id, name, region) VALUES ('c1', 'Grace', 'Lagos')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO calibration_thresholds (id, church_id, stage, accept_above, escalate_below)
         VALUES ('t1', 'c1', 'local_ai', 0.88, 0.45)",
    )
    .execute(&pool)
    .await
    .unwrap();

    let ct: CalibrationThresholds =
        sqlx::query_as("SELECT * FROM calibration_thresholds WHERE id = 't1'")
            .fetch_one(&pool)
            .await
            .unwrap();

    assert_eq!(ct.id, "t1");
    assert_eq!(ct.stage, "local_ai");
    assert!((ct.accept_above - 0.88).abs() < 1e-9);
    assert!((ct.escalate_below - 0.45).abs() < 1e-9);
    close(pool).await;
}

#[tokio::test]
async fn service_record_from_row_maps_optional_fields() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;

    sqlx::query(
        "INSERT INTO churches (id, name, region) VALUES ('c1', 'Grace', 'Lagos')",
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
        "INSERT INTO service_records
             (id, sermon_id, total_detections, auto_accepted, avg_confidence)
         VALUES ('r1', 's1', 20, 18, 0.91)",
    )
    .execute(&pool)
    .await
    .unwrap();

    let sr: ServiceRecord =
        sqlx::query_as("SELECT * FROM service_records WHERE id = 'r1'")
            .fetch_one(&pool)
            .await
            .unwrap();

    assert_eq!(sr.total_detections, 20);
    assert_eq!(sr.auto_accepted, 18);
    assert!(sr.avg_confidence.is_some_and(|v| (v - 0.91).abs() < 1e-9));
    assert!(sr.avg_processing_time_ms.is_none());
    close(pool).await;
}

// ── models: serde round-trip ──────────────────────────────────────────────────

#[test]
fn church_serde_round_trip() {
    let church = Church {
        id: "c1".into(),
        name: "Grace Church".into(),
        region: "Lagos".into(),
        installed_at: "2026-05-15T09:00:00Z".into(),
        onboarding_complete: true,
    };
    let json = serde_json::to_string(&church).unwrap();
    let decoded: Church = serde_json::from_str(&json).unwrap();
    assert_eq!(church, decoded);
}

#[test]
fn verse_serde_round_trip() {
    let verse = Verse {
        id: 1,
        book: "John".into(),
        chapter: 3,
        verse_number: 16,
        text: "For God so loved the world...".into(),
        book_order: 43,
    };
    let json = serde_json::to_string(&verse).unwrap();
    let decoded: Verse = serde_json::from_str(&json).unwrap();
    assert_eq!(verse, decoded);
}

#[test]
fn sermon_serde_round_trip() {
    let sermon = Sermon {
        id: "s1".into(),
        church_id: "c1".into(),
        title: Some("Grace and Truth".into()),
        pastor: None,
        date: "2026-05-15".into(),
        anchor_scripture: Some("John 1:17".into()),
        started_at: "2026-05-15T09:00:00Z".into(),
        ended_at: None,
    };
    let json = serde_json::to_string(&sermon).unwrap();
    let decoded: Sermon = serde_json::from_str(&json).unwrap();
    assert_eq!(sermon, decoded);
}

#[test]
fn sub_point_serde_round_trip() {
    let sp = SubPoint {
        id: "sp1".into(),
        sermon_id: "s1".into(),
        title: "Introduction".into(),
        order_index: 1,
        started_at: Some("2026-05-15T09:05:00Z".into()),
    };
    let json = serde_json::to_string(&sp).unwrap();
    let decoded: SubPoint = serde_json::from_str(&json).unwrap();
    assert_eq!(sp, decoded);
}

#[test]
fn detection_event_serde_round_trip() {
    let ev = DetectionEvent {
        id: "d1".into(),
        sermon_id: "s1".into(),
        raw_transcript: "John 3:16".into(),
        pattern_result: Some(r#"{"book":"John"}"#.into()),
        local_ai_result: None,
        cloud_ai_result: None,
        final_reference: Some("John 3:16".into()),
        confidence: 0.98,
        decision: "auto_accept".into(),
        operator_action: None,
        correct_reference: None,
        processing_time_ms: 42,
        timestamp: "2026-05-15T09:10:00Z".into(),
    };
    let json = serde_json::to_string(&ev).unwrap();
    let decoded: DetectionEvent = serde_json::from_str(&json).unwrap();
    assert_eq!(ev, decoded);
}

#[test]
fn church_settings_serde_round_trip() {
    let cs = ChurchSettings {
        church_id: "c1".into(),
        key: "preferred_translation".into(),
        value: "\"KJV\"".into(),
    };
    let json = serde_json::to_string(&cs).unwrap();
    let decoded: ChurchSettings = serde_json::from_str(&json).unwrap();
    assert_eq!(cs, decoded);
}

#[test]
fn calibration_thresholds_serde_round_trip() {
    let ct = CalibrationThresholds {
        id: "t1".into(),
        church_id: "c1".into(),
        stage: "pattern".into(),
        accept_above: 0.9,
        escalate_below: 0.5,
        updated_at: "2026-05-15T09:00:00Z".into(),
    };
    let json = serde_json::to_string(&ct).unwrap();
    let decoded: CalibrationThresholds = serde_json::from_str(&json).unwrap();
    assert_eq!(ct, decoded);
}

#[test]
fn service_record_serde_round_trip() {
    let sr = ServiceRecord {
        id: "r1".into(),
        sermon_id: "s1".into(),
        total_detections: 20,
        auto_accepted: 18,
        operator_corrected: 1,
        rejected: 1,
        avg_confidence: Some(0.91),
        avg_processing_time_ms: Some(35.5),
        created_at: "2026-05-15T09:00:00Z".into(),
    };
    let json = serde_json::to_string(&sr).unwrap();
    let decoded: ServiceRecord = serde_json::from_str(&json).unwrap();
    assert_eq!(sr, decoded);
}
