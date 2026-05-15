-- ─── acoustic_profiles ────────────────────────────────────────────────────────
-- Audio processing configuration per church environment (e.g. large reverberant
-- hall vs. small room).  Only one profile may be active at a time per church;
-- that constraint is enforced at the application layer when activating.

CREATE TABLE acoustic_profiles (
    id                TEXT    PRIMARY KEY NOT NULL,
    church_id         TEXT    NOT NULL REFERENCES churches (id) ON DELETE CASCADE,
    name              TEXT    NOT NULL,
    sample_rate       INTEGER NOT NULL DEFAULT 16000,
    chunk_duration_ms INTEGER NOT NULL DEFAULT 3000,
    noise_floor_db    REAL,
    vad_threshold     REAL    NOT NULL DEFAULT 0.5,
    is_active         INTEGER NOT NULL DEFAULT 0,
    created_at        TEXT    NOT NULL
                              DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at        TEXT    NOT NULL
                              DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),

    CHECK (is_active      IN (0, 1)),
    CHECK (vad_threshold  BETWEEN 0.0 AND 1.0),
    CHECK (sample_rate    > 0),
    CHECK (chunk_duration_ms > 0)
);

CREATE INDEX idx_acoustic_profiles_church_id ON acoustic_profiles (church_id);

-- ─── hardware_profiles ────────────────────────────────────────────────────────
-- Audio devices that have been seen or configured for a church.
-- device_id is the OS-level identifier (stable across reboots on most platforms).
-- Only one device may be preferred per church; enforced at application layer.

CREATE TABLE hardware_profiles (
    id           TEXT    PRIMARY KEY NOT NULL,
    church_id    TEXT    NOT NULL REFERENCES churches (id) ON DELETE CASCADE,
    device_name  TEXT    NOT NULL,
    device_id    TEXT    NOT NULL,
    channels     INTEGER NOT NULL DEFAULT 1,
    sample_rate  INTEGER NOT NULL DEFAULT 16000,
    is_preferred INTEGER NOT NULL DEFAULT 0,
    last_seen_at TEXT    NOT NULL
                         DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    created_at   TEXT    NOT NULL
                         DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),

    UNIQUE (church_id, device_id),

    CHECK (is_preferred IN (0, 1)),
    CHECK (channels     > 0),
    CHECK (sample_rate  > 0)
);

CREATE INDEX idx_hardware_profiles_church_id ON hardware_profiles (church_id);
