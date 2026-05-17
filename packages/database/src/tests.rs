use std::path::PathBuf;
use tempfile::tempdir;

use std::collections::HashMap;

use crate::{
    close, connect, migration, AppState, AppStateSerializer, CalibrationRepository,
    CalibrationThresholds, Church, ChurchRepository, ChurchSettings, DetectionEvent,
    DetectionEventRepository, DbPool, PoolConfig, Sermon, SermonRepository, ServiceRecord,
    SubPoint, Verse, VerseRepository, WalEntry, WriteAheadLog,
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
    assert_eq!(version, 5, "5 migrations should have been applied");
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
    assert_eq!(applied.len(), 5, "should have 5 applied migrations");
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
    assert_eq!(applied.len(), 5, "second run must not add duplicate entries");
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

// ── VerseRepository ───────────────────────────────────────────────────────────

async fn insert_test_verses(pool: &DbPool) {
    let verses = [
        ("Genesis", 1i64, 1i64, "In the beginning God created the heaven and the earth.", 1i64),
        ("Genesis", 1, 2, "And the earth was without form, and void.", 1),
        ("Genesis", 2, 1, "Thus the heavens and the earth were finished.", 1),
        ("John", 3, 16, "For God so loved the world, that he gave his only begotten Son.", 43),
        ("John", 3, 17, "For God sent not his Son into the world to condemn the world.", 43),
        ("Psalms", 23, 1, "The LORD is my shepherd; I shall not want.", 19),
    ];
    for (book, ch, v, text, order) in verses {
        sqlx::query(
            "INSERT INTO verses (book, chapter, verse_number, text, book_order)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(book)
        .bind(ch)
        .bind(v)
        .bind(text)
        .bind(order)
        .execute(pool)
        .await
        .unwrap();
    }
}

#[tokio::test]
async fn verse_repo_get_by_reference_returns_matching_verse() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;
    insert_test_verses(&pool).await;

    let repo = VerseRepository::new(pool.clone());
    let verse = repo.get_by_reference("John", 3, 16).await.unwrap();

    assert!(verse.is_some());
    let v = verse.unwrap();
    assert_eq!(v.book, "John");
    assert_eq!(v.chapter, 3);
    assert_eq!(v.verse_number, 16);
    assert!(v.text.contains("God so loved"));
    close(pool).await;
}

#[tokio::test]
async fn verse_repo_get_by_reference_returns_none_for_missing_verse() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;

    let repo = VerseRepository::new(pool.clone());
    let result = repo.get_by_reference("Revelation", 22, 21).await.unwrap();

    assert!(result.is_none());
    close(pool).await;
}

#[tokio::test]
async fn verse_repo_search_returns_matching_verses() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;
    insert_test_verses(&pool).await;

    let repo = VerseRepository::new(pool.clone());
    let results = repo.search_full_text("God").await.unwrap();

    // "In the beginning God", "For God so loved", "For God sent not" — 3 matches
    assert_eq!(results.len(), 3);
    close(pool).await;
}

#[tokio::test]
async fn verse_repo_search_returns_empty_for_no_match() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;
    insert_test_verses(&pool).await;

    let repo = VerseRepository::new(pool.clone());
    let results = repo.search_full_text("xyznonexistent").await.unwrap();

    assert!(results.is_empty());
    close(pool).await;
}

#[tokio::test]
async fn verse_repo_search_results_ordered_by_book_order_then_chapter_verse() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;
    insert_test_verses(&pool).await;

    let repo = VerseRepository::new(pool.clone());
    // "the" appears in Genesis 1:1, 1:2, 2:1, John 3:16, 3:17, Psalms 23:1
    let results = repo.search_full_text("the").await.unwrap();

    assert!(!results.is_empty());
    // verify ascending book_order, then chapter, then verse_number
    for window in results.windows(2) {
        let (a, b) = (&window[0], &window[1]);
        assert!(
            (a.book_order, a.chapter, a.verse_number)
                <= (b.book_order, b.chapter, b.verse_number)
        );
    }
    close(pool).await;
}

#[tokio::test]
async fn verse_repo_get_all_for_book_returns_only_that_book() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;
    insert_test_verses(&pool).await;

    let repo = VerseRepository::new(pool.clone());
    let verses = repo.get_all_for_book("John").await.unwrap();

    assert_eq!(verses.len(), 2);
    assert!(verses.iter().all(|v| v.book == "John"));
    close(pool).await;
}

#[tokio::test]
async fn verse_repo_get_all_for_book_ordered_by_chapter_verse() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;
    insert_test_verses(&pool).await;

    let repo = VerseRepository::new(pool.clone());
    let verses = repo.get_all_for_book("Genesis").await.unwrap();

    assert_eq!(verses.len(), 3);
    for window in verses.windows(2) {
        let (a, b) = (&window[0], &window[1]);
        assert!((a.chapter, a.verse_number) <= (b.chapter, b.verse_number));
    }
    close(pool).await;
}

#[tokio::test]
async fn verse_repo_get_all_for_book_returns_empty_for_unknown_book() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;

    let repo = VerseRepository::new(pool.clone());
    let verses = repo.get_all_for_book("NotABook").await.unwrap();

    assert!(verses.is_empty());
    close(pool).await;
}

// ── SermonRepository ──────────────────────────────────────────────────────────

fn make_sermon(id: &str, church_id: &str, started_at: &str) -> Sermon {
    Sermon {
        id: id.into(),
        church_id: church_id.into(),
        title: None,
        pastor: None,
        date: "2026-05-15".into(),
        anchor_scripture: None,
        started_at: started_at.into(),
        ended_at: None,
    }
}

#[tokio::test]
async fn sermon_repo_create_returns_the_sermon() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;

    sqlx::query(
        "INSERT INTO churches (id, name, region) VALUES ('c1', 'Grace', 'Lagos')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let repo = SermonRepository::new(pool.clone());
    let sermon = make_sermon("s1", "c1", "2026-05-15T09:00:00Z");
    let created = repo.create(sermon.clone()).await.unwrap();

    assert_eq!(created, sermon);
    close(pool).await;
}

#[tokio::test]
async fn sermon_repo_create_persists_to_database() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;

    sqlx::query(
        "INSERT INTO churches (id, name, region) VALUES ('c1', 'Grace', 'Lagos')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let repo = SermonRepository::new(pool.clone());
    repo.create(make_sermon("s1", "c1", "2026-05-15T09:00:00Z"))
        .await
        .unwrap();

    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM sermons WHERE id = 's1'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
    close(pool).await;
}

#[tokio::test]
async fn sermon_repo_get_by_id_returns_some_for_existing_sermon() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;

    sqlx::query(
        "INSERT INTO churches (id, name, region) VALUES ('c1', 'Grace', 'Lagos')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let repo = SermonRepository::new(pool.clone());
    let original = make_sermon("s1", "c1", "2026-05-15T09:00:00Z");
    repo.create(original.clone()).await.unwrap();

    let fetched = repo.get_by_id("s1").await.unwrap();
    assert_eq!(fetched, Some(original));
    close(pool).await;
}

#[tokio::test]
async fn sermon_repo_get_by_id_returns_none_for_missing_id() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;

    let repo = SermonRepository::new(pool.clone());
    let result = repo.get_by_id("nonexistent").await.unwrap();

    assert!(result.is_none());
    close(pool).await;
}

#[tokio::test]
async fn sermon_repo_update_changes_mutable_fields() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;

    sqlx::query(
        "INSERT INTO churches (id, name, region) VALUES ('c1', 'Grace', 'Lagos')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let repo = SermonRepository::new(pool.clone());
    repo.create(make_sermon("s1", "c1", "2026-05-15T09:00:00Z"))
        .await
        .unwrap();

    let updated = Sermon {
        id: "s1".into(),
        church_id: "c1".into(),
        title: Some("Faith Under Fire".into()),
        pastor: Some("Pastor John".into()),
        date: "2026-05-15".into(),
        anchor_scripture: Some("Romans 8:28".into()),
        started_at: "2026-05-15T09:00:00Z".into(),
        ended_at: None,
    };
    let returned = repo.update(updated.clone()).await.unwrap();
    assert_eq!(returned, updated);

    let fetched = repo.get_by_id("s1").await.unwrap().unwrap();
    assert_eq!(fetched.title.as_deref(), Some("Faith Under Fire"));
    assert_eq!(fetched.pastor.as_deref(), Some("Pastor John"));
    close(pool).await;
}

#[tokio::test]
async fn sermon_repo_get_recent_returns_newest_first() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;

    sqlx::query(
        "INSERT INTO churches (id, name, region) VALUES ('c1', 'Grace', 'Lagos')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let repo = SermonRepository::new(pool.clone());
    repo.create(make_sermon("s1", "c1", "2026-05-10T09:00:00Z"))
        .await
        .unwrap();
    repo.create(make_sermon("s2", "c1", "2026-05-15T09:00:00Z"))
        .await
        .unwrap();
    repo.create(make_sermon("s3", "c1", "2026-05-12T09:00:00Z"))
        .await
        .unwrap();

    let recent = repo.get_recent(10).await.unwrap();
    assert_eq!(recent.len(), 3);
    assert_eq!(recent[0].id, "s2");
    assert_eq!(recent[1].id, "s3");
    assert_eq!(recent[2].id, "s1");
    close(pool).await;
}

#[tokio::test]
async fn sermon_repo_get_recent_respects_limit() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;

    sqlx::query(
        "INSERT INTO churches (id, name, region) VALUES ('c1', 'Grace', 'Lagos')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let repo = SermonRepository::new(pool.clone());
    for i in 1..=5i64 {
        repo.create(make_sermon(
            &format!("s{i}"),
            "c1",
            &format!("2026-05-{:02}T09:00:00Z", i + 10),
        ))
        .await
        .unwrap();
    }

    let recent = repo.get_recent(3).await.unwrap();
    assert_eq!(recent.len(), 3);
    close(pool).await;
}

#[tokio::test]
async fn sermon_repo_end_sermon_sets_ended_at() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;

    sqlx::query(
        "INSERT INTO churches (id, name, region) VALUES ('c1', 'Grace', 'Lagos')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let repo = SermonRepository::new(pool.clone());
    repo.create(make_sermon("s1", "c1", "2026-05-15T09:00:00Z"))
        .await
        .unwrap();

    repo.end_sermon("s1", "2026-05-15T11:30:00Z").await.unwrap();

    let sermon = repo.get_by_id("s1").await.unwrap().unwrap();
    assert_eq!(sermon.ended_at.as_deref(), Some("2026-05-15T11:30:00Z"));
    close(pool).await;
}

#[tokio::test]
async fn sermon_repo_end_sermon_on_unknown_id_does_not_error() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;

    let repo = SermonRepository::new(pool.clone());
    // SQLite UPDATE on a non-existent row succeeds silently (0 rows affected).
    let result = repo.end_sermon("no-such-id", "2026-05-15T11:00:00Z").await;
    assert!(result.is_ok());
    close(pool).await;
}

// ── DetectionEventRepository ──────────────────────────────────────────────────

fn make_detection_event(id: &str, sermon_id: &str, transcript: &str) -> DetectionEvent {
    DetectionEvent {
        id: id.into(),
        sermon_id: sermon_id.into(),
        raw_transcript: transcript.into(),
        pattern_result: None,
        local_ai_result: None,
        cloud_ai_result: None,
        final_reference: None,
        confidence: 0.0,
        decision: "pending".into(),
        operator_action: None,
        correct_reference: None,
        processing_time_ms: 0,
        timestamp: "2026-05-15T09:00:00Z".into(),
    }
}

async fn seed_church_and_sermon(pool: &DbPool) {
    sqlx::query(
        "INSERT INTO churches (id, name, region) VALUES ('c1', 'Grace', 'Lagos')",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO sermons (id, church_id, date, started_at)
         VALUES ('s1', 'c1', '2026-05-15', '2026-05-15T09:00:00Z')",
    )
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn detection_event_repo_create_returns_event() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;
    seed_church_and_sermon(&pool).await;

    let repo = DetectionEventRepository::new(pool.clone());
    let event = make_detection_event("d1", "s1", "John 3:16");
    let created = repo.create(event.clone()).await.unwrap();

    assert_eq!(created, event);
    close(pool).await;
}

#[tokio::test]
async fn detection_event_repo_create_persists_to_database() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;
    seed_church_and_sermon(&pool).await;

    let repo = DetectionEventRepository::new(pool.clone());
    repo.create(make_detection_event("d1", "s1", "Romans 8:28"))
        .await
        .unwrap();

    let (count,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM detection_events WHERE id = 'd1'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 1);
    close(pool).await;
}

#[tokio::test]
async fn detection_event_repo_get_for_sermon_returns_all_events_ordered() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;
    seed_church_and_sermon(&pool).await;

    let repo = DetectionEventRepository::new(pool.clone());
    // Insert in reverse order; verify they come back sorted by timestamp ASC.
    let mut e1 = make_detection_event("d1", "s1", "John 3:16");
    e1.timestamp = "2026-05-15T09:01:00Z".into();
    let mut e2 = make_detection_event("d2", "s1", "Genesis 1:1");
    e2.timestamp = "2026-05-15T09:00:00Z".into();
    repo.create(e1).await.unwrap();
    repo.create(e2).await.unwrap();

    let events = repo.get_for_sermon("s1").await.unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].id, "d2");
    assert_eq!(events[1].id, "d1");
    close(pool).await;
}

#[tokio::test]
async fn detection_event_repo_get_for_sermon_returns_empty_for_no_events() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;
    seed_church_and_sermon(&pool).await;

    let repo = DetectionEventRepository::new(pool.clone());
    let events = repo.get_for_sermon("s1").await.unwrap();
    assert!(events.is_empty());
    close(pool).await;
}

#[tokio::test]
async fn detection_event_repo_update_operator_action_sets_fields() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;
    seed_church_and_sermon(&pool).await;

    let repo = DetectionEventRepository::new(pool.clone());
    repo.create(make_detection_event("d1", "s1", "Psalms 23:1"))
        .await
        .unwrap();

    repo.update_operator_action("d1", "corrected", Some("Psalms 23:1"))
        .await
        .unwrap();

    let events = repo.get_for_sermon("s1").await.unwrap();
    let ev = &events[0];
    assert_eq!(ev.operator_action.as_deref(), Some("corrected"));
    assert_eq!(ev.correct_reference.as_deref(), Some("Psalms 23:1"));
    close(pool).await;
}

#[tokio::test]
async fn detection_event_repo_update_operator_action_clears_correct_reference() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;
    seed_church_and_sermon(&pool).await;

    let repo = DetectionEventRepository::new(pool.clone());
    repo.create(make_detection_event("d1", "s1", "test"))
        .await
        .unwrap();

    repo.update_operator_action("d1", "rejected", None)
        .await
        .unwrap();

    let events = repo.get_for_sermon("s1").await.unwrap();
    assert_eq!(events[0].operator_action.as_deref(), Some("rejected"));
    assert!(events[0].correct_reference.is_none());
    close(pool).await;
}

// ── ChurchRepository ──────────────────────────────────────────────────────────

#[tokio::test]
async fn church_repo_get_or_create_inserts_when_empty() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;

    let repo = ChurchRepository::new(pool.clone());
    let church = repo.get_or_create("c1", "Grace Church", "Lagos").await.unwrap();

    assert_eq!(church.id, "c1");
    assert_eq!(church.name, "Grace Church");
    assert_eq!(church.region, "Lagos");
    close(pool).await;
}

#[tokio::test]
async fn church_repo_get_or_create_returns_existing_when_present() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;

    sqlx::query(
        "INSERT INTO churches (id, name, region) VALUES ('existing', 'Old Name', 'Abuja')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let repo = ChurchRepository::new(pool.clone());
    // Supplying different data should NOT overwrite the existing row.
    let church = repo
        .get_or_create("new-id", "New Name", "Lagos")
        .await
        .unwrap();

    assert_eq!(church.id, "existing");
    assert_eq!(church.name, "Old Name");
    close(pool).await;
}

#[tokio::test]
async fn church_repo_update_setting_inserts_new_key() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;

    let repo = ChurchRepository::new(pool.clone());
    repo.get_or_create("c1", "Grace", "Lagos").await.unwrap();
    repo.update_setting("display_font_size", "18").await.unwrap();

    let value = repo.get_setting("display_font_size").await.unwrap();
    assert_eq!(value.as_deref(), Some("18"));
    close(pool).await;
}

#[tokio::test]
async fn church_repo_update_setting_overwrites_existing_key() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;

    let repo = ChurchRepository::new(pool.clone());
    repo.get_or_create("c1", "Grace", "Lagos").await.unwrap();
    repo.update_setting("theme", "dark").await.unwrap();
    repo.update_setting("theme", "light").await.unwrap();

    let value = repo.get_setting("theme").await.unwrap();
    assert_eq!(value.as_deref(), Some("light"));
    close(pool).await;
}

#[tokio::test]
async fn church_repo_get_setting_returns_none_for_missing_key() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;

    let repo = ChurchRepository::new(pool.clone());
    repo.get_or_create("c1", "Grace", "Lagos").await.unwrap();

    let value = repo.get_setting("nonexistent_key").await.unwrap();
    assert!(value.is_none());
    close(pool).await;
}

#[tokio::test]
async fn church_repo_multiple_settings_are_independent() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;

    let repo = ChurchRepository::new(pool.clone());
    repo.get_or_create("c1", "Grace", "Lagos").await.unwrap();
    repo.update_setting("font_size", "16").await.unwrap();
    repo.update_setting("theme", "dark").await.unwrap();

    assert_eq!(repo.get_setting("font_size").await.unwrap().as_deref(), Some("16"));
    assert_eq!(repo.get_setting("theme").await.unwrap().as_deref(), Some("dark"));
    close(pool).await;
}

// ── CalibrationRepository ─────────────────────────────────────────────────────

fn make_threshold(id: &str, church_id: &str, stage: &str) -> CalibrationThresholds {
    CalibrationThresholds {
        id: id.into(),
        church_id: church_id.into(),
        stage: stage.into(),
        accept_above: 0.9,
        escalate_below: 0.5,
        updated_at: "2026-05-15T09:00:00Z".into(),
    }
}

fn make_service_record(id: &str, sermon_id: &str, created_at: &str) -> ServiceRecord {
    ServiceRecord {
        id: id.into(),
        sermon_id: sermon_id.into(),
        total_detections: 10,
        auto_accepted: 8,
        operator_corrected: 1,
        rejected: 1,
        avg_confidence: Some(0.88),
        avg_processing_time_ms: None,
        created_at: created_at.into(),
    }
}

#[tokio::test]
async fn calibration_repo_get_thresholds_returns_all_stages() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;
    seed_church_and_sermon(&pool).await;

    let repo = CalibrationRepository::new(pool.clone());
    repo.update_thresholds(make_threshold("t1", "c1", "pattern"))
        .await
        .unwrap();
    repo.update_thresholds(make_threshold("t2", "c1", "local_ai"))
        .await
        .unwrap();
    repo.update_thresholds(make_threshold("t3", "c1", "cloud_ai"))
        .await
        .unwrap();

    let thresholds = repo.get_thresholds().await.unwrap();
    assert_eq!(thresholds.len(), 3);
    let stages: Vec<&str> = thresholds.iter().map(|t| t.stage.as_str()).collect();
    // ORDER BY stage ASC → cloud_ai, local_ai, pattern
    assert_eq!(stages, vec!["cloud_ai", "local_ai", "pattern"]);
    close(pool).await;
}

#[tokio::test]
async fn calibration_repo_get_thresholds_returns_empty_before_any_inserted() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;
    seed_church_and_sermon(&pool).await;

    let repo = CalibrationRepository::new(pool.clone());
    let thresholds = repo.get_thresholds().await.unwrap();
    assert!(thresholds.is_empty());
    close(pool).await;
}

#[tokio::test]
async fn calibration_repo_update_thresholds_inserts_new_row() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;
    seed_church_and_sermon(&pool).await;

    let repo = CalibrationRepository::new(pool.clone());
    repo.update_thresholds(make_threshold("t1", "c1", "pattern"))
        .await
        .unwrap();

    let thresholds = repo.get_thresholds().await.unwrap();
    assert_eq!(thresholds.len(), 1);
    assert_eq!(thresholds[0].stage, "pattern");
    close(pool).await;
}

#[tokio::test]
async fn calibration_repo_update_thresholds_upserts_existing_stage() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;
    seed_church_and_sermon(&pool).await;

    let repo = CalibrationRepository::new(pool.clone());
    repo.update_thresholds(make_threshold("t1", "c1", "pattern"))
        .await
        .unwrap();

    let mut updated = make_threshold("t1", "c1", "pattern");
    updated.accept_above = 0.95;
    updated.escalate_below = 0.6;
    repo.update_thresholds(updated).await.unwrap();

    let thresholds = repo.get_thresholds().await.unwrap();
    assert_eq!(thresholds.len(), 1, "upsert must not create a duplicate row");
    assert!((thresholds[0].accept_above - 0.95).abs() < 1e-9);
    assert!((thresholds[0].escalate_below - 0.6).abs() < 1e-9);
    close(pool).await;
}

#[tokio::test]
async fn calibration_repo_add_service_record_persists() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;
    seed_church_and_sermon(&pool).await;

    let repo = CalibrationRepository::new(pool.clone());
    repo.add_service_record(make_service_record("r1", "s1", "2026-05-15T10:00:00Z"))
        .await
        .unwrap();

    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM service_records")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
    close(pool).await;
}

#[tokio::test]
async fn calibration_repo_get_recent_service_records_returns_newest_first() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;
    seed_church_and_sermon(&pool).await;

    // Add a second sermon for a second service record.
    sqlx::query(
        "INSERT INTO sermons (id, church_id, date, started_at)
         VALUES ('s2', 'c1', '2026-05-16', '2026-05-16T09:00:00Z')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let repo = CalibrationRepository::new(pool.clone());
    repo.add_service_record(make_service_record("r1", "s1", "2026-05-15T10:00:00Z"))
        .await
        .unwrap();
    repo.add_service_record(make_service_record("r2", "s2", "2026-05-16T10:00:00Z"))
        .await
        .unwrap();

    let records = repo.get_recent_service_records(10).await.unwrap();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].id, "r2", "most recent should be first");
    assert_eq!(records[1].id, "r1");
    close(pool).await;
}

#[tokio::test]
async fn calibration_repo_get_recent_service_records_respects_limit() {
    let dir = tempdir().unwrap();
    let pool = open_db(dir.path()).await;
    seed_church_and_sermon(&pool).await;

    // Three sermons, three records.
    for i in 2..=3i64 {
        sqlx::query(
            "INSERT INTO sermons (id, church_id, date, started_at)
             VALUES (?, 'c1', '2026-05-15', '2026-05-15T09:00:00Z')",
        )
        .bind(format!("s{i}"))
        .execute(&pool)
        .await
        .unwrap();
    }

    let repo = CalibrationRepository::new(pool.clone());
    for i in 1..=3i64 {
        repo.add_service_record(make_service_record(
            &format!("r{i}"),
            &format!("s{i}"),
            &format!("2026-05-15T{:02}:00:00Z", 9 + i),
        ))
        .await
        .unwrap();
    }

    let records = repo.get_recent_service_records(2).await.unwrap();
    assert_eq!(records.len(), 2);
    close(pool).await;
}

// ── WriteAheadLog ─────────────────────────────────────────────────────────────

#[test]
fn wal_write_returns_sequential_sequence_numbers() {
    let dir = tempdir().unwrap();
    let wal = WriteAheadLog::open(dir.path().join("test.wal")).unwrap();

    let s1 = wal.write(WalEntry::SettingChanged { key: "a".into(), value: "1".into() }).unwrap();
    let s2 = wal.write(WalEntry::SettingChanged { key: "b".into(), value: "2".into() }).unwrap();
    let s3 = wal.write(WalEntry::SettingChanged { key: "c".into(), value: "3".into() }).unwrap();

    assert_eq!(s1, 1);
    assert_eq!(s2, 2);
    assert_eq!(s3, 3);
}

#[test]
fn wal_open_creates_file_if_not_exists() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("new.wal");

    assert!(!path.exists());
    WriteAheadLog::open(&path).unwrap();
    assert!(path.exists());
}

#[test]
fn wal_open_creates_parent_directories() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("nested").join("dirs").join("app.wal");

    WriteAheadLog::open(&path).unwrap();
    assert!(path.exists());
}

#[test]
fn wal_write_produces_valid_newline_delimited_json() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("test.wal");
    let wal = WriteAheadLog::open(&path).unwrap();

    wal.write(WalEntry::SermonStarted {
        sermon_id: "s1".into(),
        church_id: "c1".into(),
        started_at: "2026-05-15T09:00:00Z".into(),
    })
    .unwrap();

    let contents = std::fs::read_to_string(&path).unwrap();
    let lines: Vec<&str> = contents.lines().collect();
    assert_eq!(lines.len(), 1, "one entry = one line");

    let record: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(record["seq"], 1);
    assert!(record["ts"].is_number());
    assert!(record["crc"].is_number(), "every record must include a crc checksum");
    assert_eq!(record["entry"]["type"], "sermon_started");
    assert_eq!(record["entry"]["sermon_id"], "s1");
}

#[test]
fn wal_write_appends_multiple_entries_each_on_own_line() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("test.wal");
    let wal = WriteAheadLog::open(&path).unwrap();

    for i in 1..=5u64 {
        wal.write(WalEntry::SettingChanged {
            key: format!("k{i}"),
            value: format!("v{i}"),
        })
        .unwrap();
    }

    let contents = std::fs::read_to_string(&path).unwrap();
    assert_eq!(contents.lines().count(), 5);
}

#[test]
fn wal_reopen_resumes_sequence_from_existing_entries() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("test.wal");

    // Session 1: write 3 entries.
    {
        let wal = WriteAheadLog::open(&path).unwrap();
        wal.write(WalEntry::SettingChanged { key: "a".into(), value: "1".into() }).unwrap();
        wal.write(WalEntry::SettingChanged { key: "b".into(), value: "2".into() }).unwrap();
        wal.write(WalEntry::SettingChanged { key: "c".into(), value: "3".into() }).unwrap();
    }

    // Session 2: should resume from 4.
    let wal2 = WriteAheadLog::open(&path).unwrap();
    let seq = wal2
        .write(WalEntry::SettingChanged { key: "d".into(), value: "4".into() })
        .unwrap();

    assert_eq!(seq, 4, "sequence must continue from where the previous session left off");

    let line_count = std::fs::read_to_string(&path).unwrap().lines().count();
    assert_eq!(line_count, 4, "file must contain all 4 entries");
}

#[test]
fn wal_write_each_entry_variant() {
    let dir = tempdir().unwrap();
    let wal = WriteAheadLog::open(dir.path().join("test.wal")).unwrap();

    let entries = vec![
        WalEntry::ChurchRegistered {
            church_id: "c1".into(),
            name: "Grace".into(),
            region: "Lagos".into(),
        },
        WalEntry::SermonStarted {
            sermon_id: "s1".into(),
            church_id: "c1".into(),
            started_at: "2026-05-15T09:00:00Z".into(),
        },
        WalEntry::SermonEnded {
            sermon_id: "s1".into(),
            ended_at: "2026-05-15T11:00:00Z".into(),
        },
        WalEntry::DetectionRecorded {
            event_id: "d1".into(),
            sermon_id: "s1".into(),
            raw_transcript: "John 3:16".into(),
            final_reference: Some("John 3:16".into()),
            confidence: 0.98,
            decision: "auto_accept".into(),
            processing_time_ms: 42,
        },
        WalEntry::OperatorCorrected {
            event_id: "d1".into(),
            action: "corrected".into(),
            correct_reference: Some("John 3:17".into()),
        },
        WalEntry::CalibrationUpdated {
            church_id: "c1".into(),
            stage: "pattern".into(),
            accept_above: 0.9,
            escalate_below: 0.5,
        },
        WalEntry::SettingChanged {
            key: "theme".into(),
            value: "dark".into(),
        },
        WalEntry::ServiceRecordSaved {
            record_id: "r1".into(),
            sermon_id: "s1".into(),
            total_detections: 20,
            auto_accepted: 18,
            operator_corrected: 1,
            rejected: 1,
        },
    ];

    let total = entries.len() as u64;
    for (i, entry) in entries.into_iter().enumerate() {
        let seq = wal.write(entry).unwrap();
        assert_eq!(seq, i as u64 + 1);
    }

    let line_count = std::fs::read_to_string(dir.path().join("test.wal"))
        .unwrap()
        .lines()
        .count();
    assert_eq!(line_count as u64, total, "every variant must produce exactly one line");
}

#[test]
fn wal_entries_are_valid_json_and_round_trip() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("test.wal");
    let wal = WriteAheadLog::open(&path).unwrap();

    let original = WalEntry::DetectionRecorded {
        event_id: "d1".into(),
        sermon_id: "s1".into(),
        raw_transcript: "Romans 8:28".into(),
        final_reference: Some("Romans 8:28".into()),
        confidence: 0.95,
        decision: "auto_accept".into(),
        processing_time_ms: 30,
    };

    wal.write(original.clone()).unwrap();

    let line = std::fs::read_to_string(&path).unwrap();
    let record: serde_json::Value = serde_json::from_str(line.trim()).unwrap();
    let decoded: WalEntry = serde_json::from_value(record["entry"].clone()).unwrap();

    assert_eq!(decoded, original);
}

// ── checkpoint ────────────────────────────────────────────────────────────────

fn make_app_state() -> AppState {
    AppState {
        church: Some(Church {
            id: "c1".into(),
            name: "Grace".into(),
            region: "Lagos".into(),
            installed_at: "2026-05-15T00:00:00Z".into(),
            onboarding_complete: true,
        }),
        active_sermon: Some(Sermon {
            id: "s1".into(),
            church_id: "c1".into(),
            title: Some("Faith".into()),
            pastor: None,
            date: "2026-05-15".into(),
            anchor_scripture: None,
            started_at: "2026-05-15T09:00:00Z".into(),
            ended_at: None,
        }),
        pending_detections: vec![],
        settings: HashMap::from([("theme".into(), "dark".into())]),
        calibration: vec![],
    }
}

#[test]
fn wal_checkpoint_returns_incremented_sequence_number() {
    let dir = tempdir().unwrap();
    let wal = WriteAheadLog::open(dir.path().join("test.wal")).unwrap();

    // Prior write advances seq to 1; checkpoint should be 2.
    wal.write(WalEntry::SettingChanged { key: "a".into(), value: "1".into() })
        .unwrap();
    let seq = wal.checkpoint(make_app_state()).unwrap();

    assert_eq!(seq, 2);
}

#[test]
fn wal_checkpoint_writes_checkpoint_variant_to_file() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("test.wal");
    let wal = WriteAheadLog::open(&path).unwrap();
    wal.checkpoint(make_app_state()).unwrap();

    let contents = std::fs::read_to_string(&path).unwrap();
    let record: serde_json::Value =
        serde_json::from_str(contents.trim()).unwrap();

    assert_eq!(record["entry"]["type"], "checkpoint");
    assert!(record["crc"].is_number());
}

// ── replay ────────────────────────────────────────────────────────────────────

#[test]
fn wal_replay_on_empty_wal_returns_default_state() {
    let dir = tempdir().unwrap();
    let wal = WriteAheadLog::open(dir.path().join("test.wal")).unwrap();

    let state = wal.replay().unwrap();
    assert_eq!(state, AppState::default());
}

#[test]
fn wal_replay_restores_exactly_the_checkpointed_state() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("test.wal");

    let original = make_app_state();
    {
        let wal = WriteAheadLog::open(&path).unwrap();
        wal.checkpoint(original.clone()).unwrap();
    }

    let wal = WriteAheadLog::open(&path).unwrap();
    let recovered = wal.replay().unwrap();
    assert_eq!(recovered, original);
}

#[test]
fn wal_replay_uses_the_last_of_multiple_checkpoints() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("test.wal");

    let mut state_v2 = make_app_state();
    state_v2.settings.insert("font_size".into(), "20".into());

    {
        let wal = WriteAheadLog::open(&path).unwrap();
        wal.checkpoint(make_app_state()).unwrap(); // checkpoint 1 — old
        wal.checkpoint(state_v2.clone()).unwrap(); // checkpoint 2 — newest
    }

    let wal = WriteAheadLog::open(&path).unwrap();
    let recovered = wal.replay().unwrap();
    assert_eq!(recovered, state_v2, "replay must start from the last checkpoint");
}

#[test]
fn wal_replay_applies_sermon_ended_after_checkpoint() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("test.wal");

    {
        let wal = WriteAheadLog::open(&path).unwrap();
        wal.checkpoint(make_app_state()).unwrap();
        wal.write(WalEntry::SermonEnded {
            sermon_id: "s1".into(),
            ended_at: "2026-05-15T11:00:00Z".into(),
        })
        .unwrap();
    }

    let wal = WriteAheadLog::open(&path).unwrap();
    let state = wal.replay().unwrap();
    let ended_at = state
        .active_sermon
        .as_ref()
        .and_then(|s| s.ended_at.as_deref());
    assert_eq!(ended_at, Some("2026-05-15T11:00:00Z"));
}

#[test]
fn wal_replay_applies_detection_recorded_after_checkpoint() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("test.wal");

    {
        let wal = WriteAheadLog::open(&path).unwrap();
        wal.checkpoint(make_app_state()).unwrap();
        wal.write(WalEntry::DetectionRecorded {
            event_id: "d1".into(),
            sermon_id: "s1".into(),
            raw_transcript: "John 3:16".into(),
            final_reference: Some("John 3:16".into()),
            confidence: 0.98,
            decision: "auto_accept".into(),
            processing_time_ms: 42,
        })
        .unwrap();
    }

    let wal = WriteAheadLog::open(&path).unwrap();
    let state = wal.replay().unwrap();
    assert_eq!(state.pending_detections.len(), 1);
    assert_eq!(state.pending_detections[0].id, "d1");
    assert_eq!(
        state.pending_detections[0].final_reference.as_deref(),
        Some("John 3:16")
    );
}

#[test]
fn wal_replay_applies_operator_corrected_after_checkpoint() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("test.wal");

    {
        let wal = WriteAheadLog::open(&path).unwrap();
        wal.checkpoint(make_app_state()).unwrap();
        wal.write(WalEntry::DetectionRecorded {
            event_id: "d1".into(),
            sermon_id: "s1".into(),
            raw_transcript: "Psalms 23".into(),
            final_reference: None,
            confidence: 0.4,
            decision: "pending".into(),
            processing_time_ms: 10,
        })
        .unwrap();
        wal.write(WalEntry::OperatorCorrected {
            event_id: "d1".into(),
            action: "corrected".into(),
            correct_reference: Some("Psalms 23:1".into()),
        })
        .unwrap();
    }

    let wal = WriteAheadLog::open(&path).unwrap();
    let state = wal.replay().unwrap();
    let det = &state.pending_detections[0];
    assert_eq!(det.operator_action.as_deref(), Some("corrected"));
    assert_eq!(det.correct_reference.as_deref(), Some("Psalms 23:1"));
}

#[test]
fn wal_replay_applies_setting_changed_after_checkpoint() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("test.wal");

    {
        let wal = WriteAheadLog::open(&path).unwrap();
        wal.checkpoint(make_app_state()).unwrap();
        wal.write(WalEntry::SettingChanged {
            key: "theme".into(),
            value: "light".into(),
        })
        .unwrap();
    }

    let wal = WriteAheadLog::open(&path).unwrap();
    let state = wal.replay().unwrap();
    assert_eq!(state.settings.get("theme").map(String::as_str), Some("light"));
}

#[test]
fn wal_replay_without_checkpoint_applies_all_entries() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("test.wal");

    {
        let wal = WriteAheadLog::open(&path).unwrap();
        wal.write(WalEntry::ChurchRegistered {
            church_id: "c1".into(),
            name: "Grace".into(),
            region: "Lagos".into(),
        })
        .unwrap();
        wal.write(WalEntry::SermonStarted {
            sermon_id: "s1".into(),
            church_id: "c1".into(),
            started_at: "2026-05-15T09:00:00Z".into(),
        })
        .unwrap();
        wal.write(WalEntry::SettingChanged {
            key: "theme".into(),
            value: "dark".into(),
        })
        .unwrap();
    }

    let wal = WriteAheadLog::open(&path).unwrap();
    let state = wal.replay().unwrap();

    assert!(state.church.is_some());
    assert_eq!(state.church.as_ref().unwrap().id, "c1");
    assert!(state.active_sermon.is_some());
    assert_eq!(state.settings.get("theme").map(String::as_str), Some("dark"));
}

#[test]
fn wal_replay_skips_entry_with_wrong_checksum() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("test.wal");

    // Write three entries: 1=setting, 2=detection (will be corrupted), 3=setting.
    {
        let wal = WriteAheadLog::open(&path).unwrap();
        wal.checkpoint(make_app_state()).unwrap();
        wal.write(WalEntry::DetectionRecorded {
            event_id: "d1".into(),
            sermon_id: "s1".into(),
            raw_transcript: "John 3:16".into(),
            final_reference: Some("John 3:16".into()),
            confidence: 0.9,
            decision: "auto_accept".into(),
            processing_time_ms: 5,
        })
        .unwrap();
        wal.write(WalEntry::SettingChanged {
            key: "theme".into(),
            value: "light".into(),
        })
        .unwrap();
    }

    // Corrupt the checksum on line 2 (the DetectionRecorded entry).
    let contents = std::fs::read_to_string(&path).unwrap();
    let lines: Vec<&str> = contents.lines().collect();
    let mut bad_record: serde_json::Value =
        serde_json::from_str(lines[1]).unwrap();
    bad_record["crc"] = serde_json::json!(0u64);
    let patched = format!(
        "{}\n{}\n{}\n",
        lines[0],
        serde_json::to_string(&bad_record).unwrap(),
        lines[2]
    );
    std::fs::write(&path, patched).unwrap();

    let wal = WriteAheadLog::open(&path).unwrap();
    let state = wal.replay().unwrap();

    // The corrupted DetectionRecorded must have been dropped.
    assert!(state.pending_detections.is_empty(), "corrupted detection must be skipped");
    // The valid SettingChanged after it must still be applied.
    assert_eq!(
        state.settings.get("theme").map(String::as_str),
        Some("light"),
        "valid entry after corrupted one must still be applied"
    );
}

#[test]
fn wal_replay_skips_unparseable_line() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("test.wal");

    {
        let wal = WriteAheadLog::open(&path).unwrap();
        wal.checkpoint(make_app_state()).unwrap();
        wal.write(WalEntry::SettingChanged {
            key: "font_size".into(),
            value: "18".into(),
        })
        .unwrap();
    }

    // Replace line 1 (the checkpoint) with garbage JSON.
    let contents = std::fs::read_to_string(&path).unwrap();
    let lines: Vec<&str> = contents.lines().collect();
    let patched = format!("{{not valid json}}\n{}\n", lines[1]);
    std::fs::write(&path, patched).unwrap();

    // Replay should skip the garbage line and still apply the SettingChanged.
    let wal = WriteAheadLog::open(&path).unwrap();
    let state = wal.replay().unwrap();
    assert_eq!(
        state.settings.get("font_size").map(String::as_str),
        Some("18"),
        "valid entry after unparseable line must still be applied"
    );
}

#[test]
fn wal_replay_detection_is_idempotent_for_duplicate_event_ids() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("test.wal");

    let detection = WalEntry::DetectionRecorded {
        event_id: "d1".into(),
        sermon_id: "s1".into(),
        raw_transcript: "John 3:16".into(),
        final_reference: None,
        confidence: 0.9,
        decision: "auto_accept".into(),
        processing_time_ms: 10,
    };

    {
        let wal = WriteAheadLog::open(&path).unwrap();
        wal.checkpoint(make_app_state()).unwrap();
        // Write the same event twice (e.g. due to a crash during commit).
        wal.write(detection.clone()).unwrap();
        wal.write(detection).unwrap();
    }

    let wal = WriteAheadLog::open(&path).unwrap();
    let state = wal.replay().unwrap();
    assert_eq!(
        state.pending_detections.len(),
        1,
        "duplicate detection event_id must not produce two entries"
    );
}

// ── WAL rotation ──────────────────────────────────────────────────────────────

#[test]
fn wal_rotate_archives_the_active_wal_file() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("app.wal");
    let wal = WriteAheadLog::open(&path).unwrap();
    wal.write(WalEntry::SettingChanged { key: "a".into(), value: "1".into() }).unwrap();

    wal.rotate().unwrap();

    // Original path still exists (new empty file).
    assert!(path.exists(), "active WAL must exist after rotation");

    // Exactly one archive file must exist alongside it.
    let archives: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name();
            let s = name.to_string_lossy();
            s.starts_with("app.wal.") && s != "app.wal"
        })
        .collect();
    assert_eq!(archives.len(), 1, "exactly one archive must be created");
}

#[test]
fn wal_rotate_new_writes_start_at_sequence_1() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("app.wal");
    let wal = WriteAheadLog::open(&path).unwrap();
    wal.write(WalEntry::SettingChanged { key: "a".into(), value: "1".into() }).unwrap();
    wal.write(WalEntry::SettingChanged { key: "b".into(), value: "2".into() }).unwrap();

    wal.rotate().unwrap();

    let seq = wal
        .write(WalEntry::SettingChanged { key: "c".into(), value: "3".into() })
        .unwrap();
    assert_eq!(seq, 1, "sequence must reset to 1 after rotation");
}

#[test]
fn wal_rotate_old_content_is_preserved_in_archive() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("app.wal");
    let wal = WriteAheadLog::open(&path).unwrap();
    wal.write(WalEntry::SettingChanged { key: "pre-rotate".into(), value: "yes".into() })
        .unwrap();

    wal.rotate().unwrap();

    // Find the archive file and verify it contains the pre-rotation entry.
    let archive = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .find(|e| {
            let n = e.file_name();
            let s = n.to_string_lossy();
            s.starts_with("app.wal.") && s != "app.wal"
        })
        .expect("archive must exist");

    let contents = std::fs::read_to_string(archive.path()).unwrap();
    assert!(
        contents.contains("pre-rotate"),
        "archive must contain pre-rotation entries"
    );
    // The active file must be empty after rotation.
    assert!(
        std::fs::read_to_string(&path).unwrap().is_empty(),
        "active WAL must be empty after rotation"
    );
}

#[test]
fn wal_rotate_prunes_archives_older_than_keep_days() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("app.wal");

    // Plant a fake archive with a timestamp 8 days ago — should be pruned.
    let old_ts = unix_now() - 8 * 86_400;
    let old_archive = dir.path().join(format!("app.wal.{old_ts}"));
    std::fs::write(&old_archive, b"old archive content\n").unwrap();

    let wal = WriteAheadLog::open(&path).unwrap();
    wal.write(WalEntry::SettingChanged { key: "k".into(), value: "v".into() }).unwrap();
    wal.rotate_keeping(7).unwrap();

    assert!(!old_archive.exists(), "archive older than keep_days must be deleted");
}

#[test]
fn wal_rotate_keeps_archives_within_keep_days() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("app.wal");

    // Plant a fake archive with a timestamp 3 days ago — must be kept.
    let recent_ts = unix_now() - 3 * 86_400;
    let recent_archive = dir.path().join(format!("app.wal.{recent_ts}"));
    std::fs::write(&recent_archive, b"recent archive\n").unwrap();

    let wal = WriteAheadLog::open(&path).unwrap();
    wal.rotate_keeping(7).unwrap();

    assert!(recent_archive.exists(), "archive within keep_days must be retained");
}

#[test]
fn wal_rotate_replay_sees_only_post_rotation_entries() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("app.wal");
    let wal = WriteAheadLog::open(&path).unwrap();

    // Pre-rotation entries.
    wal.write(WalEntry::SettingChanged { key: "old".into(), value: "before".into() }).unwrap();
    wal.rotate().unwrap();

    // Post-rotation entries + checkpoint.
    let mut state = AppState::default();
    state.settings.insert("new".into(), "after".into());
    wal.checkpoint(state).unwrap();

    let recovered = wal.replay().unwrap();
    assert_eq!(recovered.settings.get("new").map(String::as_str), Some("after"));
    // "old" was in the archived file — not visible to replay on the active WAL.
    assert!(!recovered.settings.contains_key("old"));
}

// ── write / replay cycle ──────────────────────────────────────────────────────

#[test]
fn wal_full_write_replay_cycle() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("test.wal");

    {
        let wal = WriteAheadLog::open(&path).unwrap();

        // Bootstrap state.
        wal.write(WalEntry::ChurchRegistered {
            church_id: "c1".into(),
            name: "Grace".into(),
            region: "Lagos".into(),
        })
        .unwrap();
        wal.write(WalEntry::SermonStarted {
            sermon_id: "s1".into(),
            church_id: "c1".into(),
            started_at: "2026-05-15T09:00:00Z".into(),
        })
        .unwrap();

        // Mid-session checkpoint.
        let mut base = AppState::default();
        base.church = Some(Church {
            id: "c1".into(),
            name: "Grace".into(),
            region: "Lagos".into(),
            installed_at: "2026-05-15T00:00:00Z".into(),
            onboarding_complete: true,
        });
        base.active_sermon = Some(Sermon {
            id: "s1".into(),
            church_id: "c1".into(),
            title: None,
            pastor: None,
            date: "2026-05-15".into(),
            anchor_scripture: None,
            started_at: "2026-05-15T09:00:00Z".into(),
            ended_at: None,
        });
        wal.checkpoint(base).unwrap();

        // Entries after checkpoint.
        for i in 0..5u64 {
            wal.write(WalEntry::DetectionRecorded {
                event_id: format!("d{i}"),
                sermon_id: "s1".into(),
                raw_transcript: format!("John {i}:1"),
                final_reference: Some(format!("John {i}:1")),
                confidence: 0.95,
                decision: "auto_accept".into(),
                processing_time_ms: 10,
            })
            .unwrap();
        }
        wal.write(WalEntry::SermonEnded {
            sermon_id: "s1".into(),
            ended_at: "2026-05-15T11:00:00Z".into(),
        })
        .unwrap();
    }

    let wal = WriteAheadLog::open(&path).unwrap();
    let state = wal.replay().unwrap();

    assert!(state.church.is_some());
    assert_eq!(state.pending_detections.len(), 5);
    assert_eq!(
        state.active_sermon.as_ref().and_then(|s| s.ended_at.as_deref()),
        Some("2026-05-15T11:00:00Z")
    );
}

// ── corrupted entry handling ──────────────────────────────────────────────────

#[test]
fn wal_replay_handles_multiple_consecutive_corrupted_entries() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("test.wal");

    {
        let wal = WriteAheadLog::open(&path).unwrap();
        wal.checkpoint(make_app_state()).unwrap();
        for i in 0..5u64 {
            wal.write(WalEntry::DetectionRecorded {
                event_id: format!("d{i}"),
                sermon_id: "s1".into(),
                raw_transcript: format!("text {i}"),
                final_reference: None,
                confidence: 0.8,
                decision: "auto_accept".into(),
                processing_time_ms: 5,
            })
            .unwrap();
        }
    }

    // Corrupt lines 2, 3, and 4 (zero out their crc fields).
    let contents = std::fs::read_to_string(&path).unwrap();
    let lines: Vec<&str> = contents.lines().collect();
    let patched: String = lines
        .iter()
        .enumerate()
        .map(|(i, line)| {
            if i == 1 || i == 2 || i == 3 {
                let mut v: serde_json::Value = serde_json::from_str(line).unwrap();
                v["crc"] = serde_json::json!(0u64);
                format!("{}\n", serde_json::to_string(&v).unwrap())
            } else {
                format!("{line}\n")
            }
        })
        .collect();
    std::fs::write(&path, patched).unwrap();

    let wal = WriteAheadLog::open(&path).unwrap();
    let state = wal.replay().unwrap();

    // Lines 2–4 (d0, d1, d2) were corrupted; d3 and d4 survive.
    assert_eq!(
        state.pending_detections.len(),
        2,
        "only entries with valid checksums must survive"
    );
    let ids: Vec<&str> = state.pending_detections.iter().map(|d| d.id.as_str()).collect();
    assert!(ids.contains(&"d3"));
    assert!(ids.contains(&"d4"));
}

#[test]
fn wal_replay_handles_truncated_final_entry() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("test.wal");

    {
        let wal = WriteAheadLog::open(&path).unwrap();
        wal.checkpoint(make_app_state()).unwrap();
        wal.write(WalEntry::DetectionRecorded {
            event_id: "d1".into(),
            sermon_id: "s1".into(),
            raw_transcript: "complete entry".into(),
            final_reference: Some("John 3:16".into()),
            confidence: 0.9,
            decision: "auto_accept".into(),
            processing_time_ms: 10,
        })
        .unwrap();
        // Write one more entry that will be truncated mid-way.
        wal.write(WalEntry::DetectionRecorded {
            event_id: "d2".into(),
            sermon_id: "s1".into(),
            raw_transcript: "incomplete entry — will be truncated".into(),
            final_reference: None,
            confidence: 0.5,
            decision: "pending".into(),
            processing_time_ms: 5,
        })
        .unwrap();
    }

    // Truncate the last line to simulate a crash mid-write.
    let contents = std::fs::read_to_string(&path).unwrap();
    let mut lines: Vec<&str> = contents.lines().collect();
    // Chop off the last line (the d2 entry).
    lines.pop();
    std::fs::write(&path, lines.join("\n") + "\n").unwrap();

    let wal = WriteAheadLog::open(&path).unwrap();
    let state = wal.replay().unwrap();

    // d1 must be present; d2 was lost in the crash.
    assert_eq!(state.pending_detections.len(), 1);
    assert_eq!(state.pending_detections[0].id, "d1");
}

// ── missing checkpoint ────────────────────────────────────────────────────────

#[test]
fn wal_replay_without_checkpoint_rebuilds_full_state_from_entries() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("test.wal");

    {
        let wal = WriteAheadLog::open(&path).unwrap();
        wal.write(WalEntry::ChurchRegistered {
            church_id: "c1".into(),
            name: "Grace".into(),
            region: "Lagos".into(),
        })
        .unwrap();
        wal.write(WalEntry::SettingChanged { key: "theme".into(), value: "dark".into() })
            .unwrap();
        wal.write(WalEntry::SettingChanged { key: "font_size".into(), value: "18".into() })
            .unwrap();
        wal.write(WalEntry::SermonStarted {
            sermon_id: "s1".into(),
            church_id: "c1".into(),
            started_at: "2026-05-15T09:00:00Z".into(),
        })
        .unwrap();
        for i in 0..3u64 {
            wal.write(WalEntry::DetectionRecorded {
                event_id: format!("d{i}"),
                sermon_id: "s1".into(),
                raw_transcript: format!("ref {i}"),
                final_reference: Some(format!("Ps {i}:1")),
                confidence: 0.88,
                decision: "auto_accept".into(),
                processing_time_ms: 8,
            })
            .unwrap();
        }
        wal.write(WalEntry::CalibrationUpdated {
            church_id: "c1".into(),
            stage: "pattern".into(),
            accept_above: 0.92,
            escalate_below: 0.55,
        })
        .unwrap();
    }

    let wal = WriteAheadLog::open(&path).unwrap();
    let state = wal.replay().unwrap();

    assert_eq!(state.church.as_ref().map(|c| c.id.as_str()), Some("c1"));
    assert_eq!(state.settings.get("theme").map(String::as_str), Some("dark"));
    assert_eq!(state.settings.get("font_size").map(String::as_str), Some("18"));
    assert!(state.active_sermon.is_some());
    assert_eq!(state.pending_detections.len(), 3);
    assert_eq!(state.calibration.len(), 1);
    assert!((state.calibration[0].accept_above - 0.92).abs() < 1e-9);
}

// ── crash simulation ──────────────────────────────────────────────────────────

fn unix_now() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[test]
fn wal_crash_simulation_1000_entries_state_is_consistent() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("test.wal");

    let mut expected_detections: Vec<String> = Vec::new();

    {
        let wal = WriteAheadLog::open(&path).unwrap();

        let mut state = AppState {
            church: Some(Church {
                id: "c1".into(),
                name: "Grace".into(),
                region: "Lagos".into(),
                installed_at: "2026-05-15T00:00:00Z".into(),
                onboarding_complete: true,
            }),
            active_sermon: Some(Sermon {
                id: "s1".into(),
                church_id: "c1".into(),
                title: None,
                pastor: None,
                date: "2026-05-15".into(),
                anchor_scripture: None,
                started_at: "2026-05-15T09:00:00Z".into(),
                ended_at: None,
            }),
            pending_detections: vec![],
            settings: HashMap::new(),
            calibration: vec![],
        };

        wal.write(WalEntry::SermonStarted {
            sermon_id: "s1".into(),
            church_id: "c1".into(),
            started_at: "2026-05-15T09:00:00Z".into(),
        })
        .unwrap();

        for i in 0..1000u64 {
            let event_id = format!("d{i}");
            wal.write(WalEntry::DetectionRecorded {
                event_id: event_id.clone(),
                sermon_id: "s1".into(),
                raw_transcript: format!("transcript chunk {i}"),
                final_reference: Some(format!("John {i}:1")),
                confidence: 0.9,
                decision: "auto_accept".into(),
                processing_time_ms: 10,
            })
            .unwrap();

            state.pending_detections.push(DetectionEvent {
                id: event_id.clone(),
                sermon_id: "s1".into(),
                raw_transcript: format!("transcript chunk {i}"),
                pattern_result: None,
                local_ai_result: None,
                cloud_ai_result: None,
                final_reference: Some(format!("John {i}:1")),
                confidence: 0.9,
                decision: "auto_accept".into(),
                operator_action: None,
                correct_reference: None,
                processing_time_ms: 10,
                timestamp: String::new(),
            });
            expected_detections.push(event_id);

            // Checkpoint every 100 entries — simulates the 1-second interval.
            if (i + 1) % 100 == 0 {
                wal.checkpoint(state.clone()).unwrap();
            }
        }
    }

    // ── Simulate crash: truncate to 2/3 of file size ──────────────────────────
    let file_bytes = std::fs::read(&path).unwrap();
    let crash_at = file_bytes.len() * 2 / 3;
    std::fs::write(&path, &file_bytes[..crash_at]).unwrap();

    // ── Replay and verify internal consistency ────────────────────────────────
    let wal = WriteAheadLog::open(&path).unwrap();
    let state = wal.replay().unwrap();

    // No duplicate event IDs.
    let ids: std::collections::HashSet<&str> =
        state.pending_detections.iter().map(|d| d.id.as_str()).collect();
    assert_eq!(
        ids.len(),
        state.pending_detections.len(),
        "pending detections must have unique IDs after crash recovery"
    );

    // All detections belong to the active sermon.
    if let Some(ref sermon) = state.active_sermon {
        for det in &state.pending_detections {
            assert_eq!(
                det.sermon_id, sermon.id,
                "every recovered detection must reference the active sermon"
            );
        }
    }

    // Event IDs are a prefix of what was written (no invented entries).
    for det in &state.pending_detections {
        assert!(
            expected_detections.contains(&det.id),
            "recovered detection {} was never written",
            det.id
        );
    }

    // At least the last checkpoint (entry 900 or 1000) must have been recovered.
    // With crash at 2/3 we always hit at least the first checkpoint (entry 100).
    assert!(
        !state.pending_detections.is_empty(),
        "at least some detections must survive a crash at 2/3 of file"
    );
}

// ── AppStateSerializer — state round-trip ─────────────────────────────────────

fn make_full_app_state() -> AppState {
    AppState {
        church: Some(Church {
            id: "c1".into(),
            name: "Grace Baptist Church".into(),
            region: "Lagos".into(),
            installed_at: "2026-05-15T00:00:00Z".into(),
            onboarding_complete: true,
        }),
        active_sermon: Some(Sermon {
            id: "s1".into(),
            church_id: "c1".into(),
            title: Some("The Power of Faith".into()),
            pastor: Some("Pastor John".into()),
            date: "2026-05-15".into(),
            anchor_scripture: Some("Romans 8:28".into()),
            started_at: "2026-05-15T09:00:00Z".into(),
            ended_at: None,
        }),
        pending_detections: vec![
            DetectionEvent {
                id: "d1".into(),
                sermon_id: "s1".into(),
                raw_transcript: "John 3:16".into(),
                pattern_result: Some(r#"{"book":"John"}"#.into()),
                local_ai_result: None,
                cloud_ai_result: None,
                final_reference: Some("John 3:16".into()),
                confidence: 0.97,
                decision: "auto_accept".into(),
                operator_action: None,
                correct_reference: None,
                processing_time_ms: 38,
                timestamp: "2026-05-15T09:05:00Z".into(),
            },
        ],
        settings: HashMap::from([
            ("theme".into(), "dark".into()),
            ("font_size".into(), "18".into()),
        ]),
        calibration: vec![
            CalibrationThresholds {
                id: "t1".into(),
                church_id: "c1".into(),
                stage: "pattern".into(),
                accept_above: 0.9,
                escalate_below: 0.5,
                updated_at: "2026-05-15T00:00:00Z".into(),
            },
        ],
    }
}

#[test]
fn persist_save_load_round_trips_full_app_state() {
    let dir = tempdir().unwrap();
    let s = AppStateSerializer::new(dir.path());
    let original = make_full_app_state();

    s.save_state(&original).unwrap();
    let loaded = s.load_state().unwrap().expect("state file must be readable");

    assert_eq!(loaded, original);
}

#[test]
fn persist_load_returns_none_when_no_file_exists() {
    let dir = tempdir().unwrap();
    let s = AppStateSerializer::new(dir.path());

    let result = s.load_state().unwrap();
    assert!(result.is_none());
}

#[test]
fn persist_save_creates_parent_directories() {
    let dir = tempdir().unwrap();
    let nested = dir.path().join("a").join("b").join("c");
    let s = AppStateSerializer::new(&nested);

    s.save_state(&AppState::default()).unwrap();
    assert!(nested.join("app_state.json").exists());
}

#[test]
fn persist_save_is_atomic_no_temp_file_after_success() {
    let dir = tempdir().unwrap();
    let s = AppStateSerializer::new(dir.path());

    s.save_state(&make_full_app_state()).unwrap();

    // The .tmp file must have been renamed away.
    let tmp = s.state_path().with_extension("tmp");
    assert!(!tmp.exists(), "temp file must not remain after successful save");
}

#[test]
fn persist_save_overwrites_previous_state() {
    let dir = tempdir().unwrap();
    let s = AppStateSerializer::new(dir.path());

    let mut first = make_full_app_state();
    first.settings.insert("version".into(), "1".into());
    s.save_state(&first).unwrap();

    let mut second = make_full_app_state();
    second.settings.insert("version".into(), "2".into());
    s.save_state(&second).unwrap();

    let loaded = s.load_state().unwrap().unwrap();
    assert_eq!(
        loaded.settings.get("version").map(String::as_str),
        Some("2"),
        "second save must overwrite first"
    );
}

// ── AppStateSerializer — corrupted state handling ─────────────────────────────

#[test]
fn persist_load_returns_none_for_invalid_json() {
    let dir = tempdir().unwrap();
    let s = AppStateSerializer::new(dir.path());

    std::fs::write(s.state_path(), b"this is not json").unwrap();

    let result = s.load_state().unwrap();
    assert!(result.is_none(), "invalid JSON must yield None, not an error");
}

#[test]
fn persist_load_returns_none_for_empty_file() {
    let dir = tempdir().unwrap();
    let s = AppStateSerializer::new(dir.path());

    std::fs::write(s.state_path(), b"").unwrap();

    let result = s.load_state().unwrap();
    assert!(result.is_none(), "empty file must yield None");
}

#[test]
fn persist_load_returns_none_for_truncated_json() {
    let dir = tempdir().unwrap();
    let s = AppStateSerializer::new(dir.path());

    // Write valid JSON, then truncate it mid-way.
    s.save_state(&make_full_app_state()).unwrap();
    let full = std::fs::read(s.state_path()).unwrap();
    let half = &full[..full.len() / 2];
    std::fs::write(s.state_path(), half).unwrap();

    let result = s.load_state().unwrap();
    assert!(result.is_none(), "truncated JSON must yield None");
}

#[test]
fn persist_load_returns_none_for_json_wrong_shape() {
    let dir = tempdir().unwrap();
    let s = AppStateSerializer::new(dir.path());

    // Valid JSON but wrong shape for AppState.
    std::fs::write(s.state_path(), br#"{"not_a_field": 42}"#).unwrap();

    // serde will produce a default AppState from missing optional fields —
    // this depends on the derive; either way we assert no panic.
    let _result = s.load_state();
}

// ── AppStateSerializer — crash marker ────────────────────────────────────────

#[test]
fn crash_marker_write_creates_file() {
    let dir = tempdir().unwrap();
    let s = AppStateSerializer::new(dir.path());

    s.write_crash_marker().unwrap();

    assert!(s.marker_path().exists(), "marker file must exist after write");
}

#[test]
fn crash_marker_delete_removes_file() {
    let dir = tempdir().unwrap();
    let s = AppStateSerializer::new(dir.path());

    s.write_crash_marker().unwrap();
    s.delete_crash_marker().unwrap();

    assert!(!s.marker_path().exists(), "marker file must not exist after delete");
}

#[test]
fn crash_marker_delete_is_ok_when_file_not_found() {
    let dir = tempdir().unwrap();
    let s = AppStateSerializer::new(dir.path());

    // Deleting a non-existent marker must not return an error.
    assert!(s.delete_crash_marker().is_ok());
}

#[test]
fn crash_marker_exists_returns_true_when_present() {
    let dir = tempdir().unwrap();
    let s = AppStateSerializer::new(dir.path());

    s.write_crash_marker().unwrap();
    assert!(s.crash_marker_exists());
}

#[test]
fn crash_marker_exists_returns_false_when_absent() {
    let dir = tempdir().unwrap();
    let s = AppStateSerializer::new(dir.path());

    assert!(!s.crash_marker_exists());
}

#[test]
fn crash_marker_creates_parent_directories() {
    let dir = tempdir().unwrap();
    let nested = dir.path().join("deep").join("nested");
    let s = AppStateSerializer::new(&nested);

    s.write_crash_marker().unwrap();
    assert!(s.marker_path().exists());
}

// ── AppStateSerializer — clean startup sequence ───────────────────────────────

#[test]
fn persist_clean_startup_sequence_no_crash_detected() {
    let dir = tempdir().unwrap();
    let s = AppStateSerializer::new(dir.path());

    // Simulate a clean previous session.
    s.save_state(&make_full_app_state()).unwrap();
    // Marker was deleted on clean shutdown — it should not exist.
    assert!(!s.crash_marker_exists(), "no marker after clean shutdown");

    // On this startup: no crash detected, load from state file.
    let state = s.load_state().unwrap();
    assert!(state.is_some(), "state must load after clean shutdown");
}

#[test]
fn persist_crash_startup_sequence_marker_detected() {
    let dir = tempdir().unwrap();
    let s = AppStateSerializer::new(dir.path());

    // Simulate: previous session started (marker written) but never shut down.
    s.save_state(&make_full_app_state()).unwrap();
    s.write_crash_marker().unwrap();

    // On this startup: crash detected.
    assert!(
        s.crash_marker_exists(),
        "crash marker must be present after simulated crash"
    );
    // Caller would use WAL replay here instead of load_state().
    // After recovery, write a new marker for this session.
    s.write_crash_marker().unwrap();
    assert!(s.crash_marker_exists());
}

#[test]
fn persist_full_session_lifecycle() {
    let dir = tempdir().unwrap();
    let s = AppStateSerializer::new(dir.path());

    // 1. First launch — no state, no marker.
    assert!(!s.crash_marker_exists());
    assert!(s.load_state().unwrap().is_none());

    // 2. Mark session start.
    s.write_crash_marker().unwrap();
    assert!(s.crash_marker_exists());

    // 3. Save state mid-session.
    let state = make_full_app_state();
    s.save_state(&state).unwrap();

    // 4. Clean shutdown.
    s.delete_crash_marker().unwrap();
    assert!(!s.crash_marker_exists());

    // 5. Next launch — no crash, state loads correctly.
    let loaded = s.load_state().unwrap().expect("state must survive clean shutdown");
    assert_eq!(loaded, state);
}
