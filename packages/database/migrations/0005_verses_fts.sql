-- FTS5 virtual table for KJV verse quotation matching.
--
-- Uses the porter stemmer so "loved"/"love", "gave"/"give", "believeth"/"believe"
-- all match correctly against spoken transcript text.
--
-- content='verses' + content_rowid='id' means FTS5 reads actual text back from
-- the verses table (no duplication). The triggers below keep the index in sync.

CREATE VIRTUAL TABLE IF NOT EXISTS verses_fts USING fts5(
    book     UNINDEXED,
    chapter  UNINDEXED,
    verse_number UNINDEXED,
    text,
    content      = 'verses',
    content_rowid = 'id',
    tokenize     = 'porter unicode61'
);

-- Bulk-populate for existing installs (no-op on fresh installs where verses
-- table is still empty; triggers handle inserts from that point on).
INSERT INTO verses_fts (rowid, book, chapter, verse_number, text)
    SELECT id, book, chapter, verse_number, text FROM verses;

-- ── Sync triggers ─────────────────────────────────────────────────────────────

CREATE TRIGGER verses_ai AFTER INSERT ON verses BEGIN
    INSERT INTO verses_fts (rowid, book, chapter, verse_number, text)
    VALUES (new.id, new.book, new.chapter, new.verse_number, new.text);
END;

CREATE TRIGGER verses_ad AFTER DELETE ON verses BEGIN
    INSERT INTO verses_fts (verses_fts, rowid, book, chapter, verse_number, text)
    VALUES ('delete', old.id, old.book, old.chapter, old.verse_number, old.text);
END;

CREATE TRIGGER verses_au AFTER UPDATE ON verses BEGIN
    INSERT INTO verses_fts (verses_fts, rowid, book, chapter, verse_number, text)
    VALUES ('delete', old.id, old.book, old.chapter, old.verse_number, old.text);
    INSERT INTO verses_fts (rowid, book, chapter, verse_number, text)
    VALUES (new.id, new.book, new.chapter, new.verse_number, new.text);
END;
