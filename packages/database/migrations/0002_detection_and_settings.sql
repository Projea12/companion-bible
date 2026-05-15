-- ─── detection_events ─────────────────────────────────────────────────────────
-- One row per transcript chunk processed by the detection pipeline.
-- JSON columns store the raw output from each stage; only the columns that
-- ran are non-NULL.  confidence is in [0.0, 1.0].

CREATE TABLE detection_events (
    id                 TEXT PRIMARY KEY NOT NULL,
    sermon_id          TEXT NOT NULL REFERENCES sermons (id) ON DELETE CASCADE,
    raw_transcript     TEXT NOT NULL,
    pattern_result     TEXT,           -- JSON or NULL
    local_ai_result    TEXT,           -- JSON or NULL
    cloud_ai_result    TEXT,           -- JSON or NULL
    final_reference    TEXT,
    confidence         REAL NOT NULL DEFAULT 0.0,
    decision           TEXT NOT NULL,
    operator_action    TEXT,
    correct_reference  TEXT,
    processing_time_ms INTEGER NOT NULL DEFAULT 0,
    timestamp          TEXT NOT NULL
                            DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),

    CHECK (confidence >= 0.0 AND confidence <= 1.0),
    CHECK (processing_time_ms >= 0)
);

CREATE INDEX idx_detection_events_sermon_id       ON detection_events (sermon_id);
CREATE INDEX idx_detection_events_timestamp       ON detection_events (timestamp);
CREATE INDEX idx_detection_events_final_reference ON detection_events (final_reference)
    WHERE final_reference IS NOT NULL;

-- ─── app_state ────────────────────────────────────────────────────────────────
-- Global key/value store for application-level state (e.g. last_opened_sermon,
-- display_mode).  value is always valid JSON.

CREATE TABLE app_state (
    key        TEXT PRIMARY KEY NOT NULL,
    value      TEXT NOT NULL,
    updated_at TEXT NOT NULL
                   DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

-- ─── church_settings ──────────────────────────────────────────────────────────
-- Per-church configuration (e.g. preferred_translation, display_font_size).
-- Composite primary key prevents duplicate keys for the same church.

CREATE TABLE church_settings (
    church_id TEXT NOT NULL REFERENCES churches (id) ON DELETE CASCADE,
    key       TEXT NOT NULL,
    value     TEXT NOT NULL,

    PRIMARY KEY (church_id, key)
);

CREATE INDEX idx_church_settings_church_id ON church_settings (church_id);
