-- Learned transcription corrections collected from operator feedback.
--
-- When an operator corrects a detected reference (e.g. the system heard
-- "Corenten" but the correct book was "Corinthians"), the wrong→right word
-- pair is stored here so future sessions can correct it automatically without
-- waiting for a static CORRECTIONS table update.
--
-- Unique on (wrong_form, church_id): the same wrong form may map to different
-- corrections at different churches (rare, but possible for book name clashes).

CREATE TABLE IF NOT EXISTS learned_corrections (
    id          TEXT    PRIMARY KEY NOT NULL,
    wrong_form  TEXT    NOT NULL,
    correct_form TEXT   NOT NULL,
    church_id   TEXT    REFERENCES churches(id) ON DELETE CASCADE,
    count       INTEGER NOT NULL DEFAULT 1,
    created_at  TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at  TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    UNIQUE(wrong_form, church_id)
);

CREATE INDEX IF NOT EXISTS idx_learned_corrections_wrong
    ON learned_corrections(wrong_form);

CREATE INDEX IF NOT EXISTS idx_learned_corrections_church
    ON learned_corrections(church_id);
