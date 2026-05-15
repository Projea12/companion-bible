-- ─── churches ─────────────────────────────────────────────────────────────────
-- One row per installation.  A device belongs to a single church.

CREATE TABLE churches (
    id                   TEXT    PRIMARY KEY NOT NULL,
    name                 TEXT    NOT NULL,
    region               TEXT    NOT NULL,
    installed_at         TEXT    NOT NULL
                                 DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    onboarding_complete  INTEGER NOT NULL DEFAULT 0,

    CHECK (onboarding_complete IN (0, 1))
);

-- ─── verses ───────────────────────────────────────────────────────────────────
-- Mirrors kjv.json; populated once at first launch from the bundled data file.
-- book_order matches the canonical Bible order (Genesis = 1 … Revelation = 66).

CREATE TABLE verses (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    book         TEXT    NOT NULL,
    chapter      INTEGER NOT NULL,
    verse_number INTEGER NOT NULL,
    text         TEXT    NOT NULL,
    book_order   INTEGER NOT NULL,

    UNIQUE (book, chapter, verse_number)
);

CREATE INDEX idx_verses_book_chapter ON verses (book, chapter);
CREATE INDEX idx_verses_book_order   ON verses (book_order);

-- ─── sermons ──────────────────────────────────────────────────────────────────
-- Each sermon / listening session belongs to the local church.

CREATE TABLE sermons (
    id               TEXT PRIMARY KEY NOT NULL,
    church_id        TEXT NOT NULL REFERENCES churches (id) ON DELETE CASCADE,
    title            TEXT,
    pastor           TEXT,
    date             TEXT NOT NULL,
    anchor_scripture TEXT,
    started_at       TEXT NOT NULL
                          DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    ended_at         TEXT
);

CREATE INDEX idx_sermons_church_id ON sermons (church_id);
CREATE INDEX idx_sermons_date      ON sermons (date);

-- ─── sub_points ───────────────────────────────────────────────────────────────
-- Ordered sections within a sermon (e.g. "Point 1 — Faith").

CREATE TABLE sub_points (
    id          TEXT    PRIMARY KEY NOT NULL,
    sermon_id   TEXT    NOT NULL REFERENCES sermons (id) ON DELETE CASCADE,
    title       TEXT    NOT NULL,
    order_index INTEGER NOT NULL,
    started_at  TEXT
);

CREATE INDEX idx_sub_points_sermon_id ON sub_points (sermon_id);
