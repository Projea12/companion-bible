-- ─── calibration_thresholds ───────────────────────────────────────────────────
-- Per-church, per-stage confidence thresholds.
-- accept_above: auto-accept detection if confidence >= this value.
-- escalate_below: escalate to the next stage if confidence < this value.
-- The band between escalate_below and accept_above triggers manual review.

CREATE TABLE calibration_thresholds (
    id             TEXT PRIMARY KEY NOT NULL,
    church_id      TEXT NOT NULL REFERENCES churches (id) ON DELETE CASCADE,
    stage          TEXT NOT NULL,
    accept_above   REAL NOT NULL DEFAULT 0.9,
    escalate_below REAL NOT NULL DEFAULT 0.5,
    updated_at     TEXT NOT NULL
                        DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),

    UNIQUE (church_id, stage),

    CHECK (stage IN ('pattern', 'local_ai', 'cloud_ai')),
    CHECK (accept_above  BETWEEN 0.0 AND 1.0),
    CHECK (escalate_below BETWEEN 0.0 AND 1.0),
    CHECK (accept_above > escalate_below)
);

CREATE INDEX idx_calibration_thresholds_church_id ON calibration_thresholds (church_id);

-- ─── service_records ──────────────────────────────────────────────────────────
-- One summary row per sermon: aggregate detection pipeline performance metrics.

CREATE TABLE service_records (
    id                    TEXT    PRIMARY KEY NOT NULL,
    sermon_id             TEXT    NOT NULL REFERENCES sermons (id) ON DELETE CASCADE,
    total_detections      INTEGER NOT NULL DEFAULT 0,
    auto_accepted         INTEGER NOT NULL DEFAULT 0,
    operator_corrected    INTEGER NOT NULL DEFAULT 0,
    rejected              INTEGER NOT NULL DEFAULT 0,
    avg_confidence        REAL,
    avg_processing_time_ms REAL,
    created_at            TEXT    NOT NULL
                                  DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),

    UNIQUE (sermon_id),

    CHECK (total_detections   >= 0),
    CHECK (auto_accepted      >= 0),
    CHECK (operator_corrected >= 0),
    CHECK (rejected           >= 0)
);

CREATE INDEX idx_service_records_sermon_id ON service_records (sermon_id);

-- ─── operator_patterns ────────────────────────────────────────────────────────
-- Custom text patterns defined by operators for their church context
-- (e.g. local abbreviations, dialect spellings).
-- Higher priority value = checked first in the pattern pipeline.

CREATE TABLE operator_patterns (
    id         TEXT    PRIMARY KEY NOT NULL,
    church_id  TEXT    NOT NULL REFERENCES churches (id) ON DELETE CASCADE,
    pattern    TEXT    NOT NULL,
    book_name  TEXT    NOT NULL,
    match_type TEXT    NOT NULL DEFAULT 'exact',
    priority   INTEGER NOT NULL DEFAULT 0,
    is_active  INTEGER NOT NULL DEFAULT 1,
    created_at TEXT    NOT NULL
                       DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),

    UNIQUE (church_id, pattern),

    CHECK (match_type IN ('exact', 'contains', 'regex')),
    CHECK (is_active  IN (0, 1)),
    CHECK (priority   >= 0)
);

CREATE INDEX idx_operator_patterns_church_id ON operator_patterns (church_id);
CREATE INDEX idx_operator_patterns_active    ON operator_patterns (church_id, is_active)
    WHERE is_active = 1;
