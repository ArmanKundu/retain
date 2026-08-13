-- ---------------------------------------------------------------------------
-- Resources: study material you supply.
--
-- The study design, past papers, class notes — anything Retain should be able
-- to ground an answer in. Stored as extracted plain text rather than the
-- original file: the app needs to read and search it, not re-render it, and
-- keeping a copy of every PDF would bloat the database and the export for no
-- benefit. The original stays wherever you keep it.
-- ---------------------------------------------------------------------------

CREATE TABLE resources (
    id          INTEGER PRIMARY KEY,
    subject_id  INTEGER          REFERENCES subjects(id) ON DELETE SET NULL,
    title       TEXT    NOT NULL,
    -- Drives how a resource is used, not just how it's labelled: a study design
    -- is authoritative about what's examinable, a past paper is an example of
    -- how it's asked. Prompts treat them differently.
    kind        TEXT    NOT NULL CHECK (kind IN ('study_design', 'past_paper', 'notes', 'other')),
    source      TEXT,                       -- original filename, or how it arrived
    content     TEXT    NOT NULL,           -- extracted plain text
    word_count  INTEGER NOT NULL DEFAULT 0,
    added_at    TEXT    NOT NULL
);
CREATE INDEX idx_resources_subject ON resources(subject_id, kind);

-- Retrieval works on chunks rather than whole documents. A past paper is far
-- too long to put in a prompt, and the useful part is usually a page or two.
CREATE TABLE resource_chunks (
    id          INTEGER PRIMARY KEY,
    resource_id INTEGER NOT NULL REFERENCES resources(id) ON DELETE CASCADE,
    ordinal     INTEGER NOT NULL,
    content     TEXT    NOT NULL
);
CREATE INDEX idx_chunks_resource ON resource_chunks(resource_id, ordinal);

-- Full-text index over the chunks. `content=` makes this an external-content
-- table: FTS stores only the index, not a second copy of the text.
CREATE VIRTUAL TABLE resource_chunks_fts USING fts5(
    content,
    content = 'resource_chunks',
    content_rowid = 'id',
    tokenize = 'porter unicode61'
);

-- External-content FTS tables do not stay in sync on their own; these triggers
-- are what keep the index honest. Without them a deleted resource stays
-- searchable forever and starts appearing as context for questions about
-- something you removed.
CREATE TRIGGER resource_chunks_ai AFTER INSERT ON resource_chunks BEGIN
    INSERT INTO resource_chunks_fts(rowid, content) VALUES (new.id, new.content);
END;
CREATE TRIGGER resource_chunks_ad AFTER DELETE ON resource_chunks BEGIN
    INSERT INTO resource_chunks_fts(resource_chunks_fts, rowid, content)
    VALUES ('delete', old.id, old.content);
END;
CREATE TRIGGER resource_chunks_au AFTER UPDATE ON resource_chunks BEGIN
    INSERT INTO resource_chunks_fts(resource_chunks_fts, rowid, content)
    VALUES ('delete', old.id, old.content);
    INSERT INTO resource_chunks_fts(rowid, content) VALUES (new.id, new.content);
END;

-- ---------------------------------------------------------------------------
-- Library: everything the AI has produced for you, kept.
--
-- Previously an AI answer existed only until you navigated away. Notes you
-- asked for, practice questions, weekly reviews — all gone. Every generation
-- now lands here, so it can be found again, exported and printed.
-- ---------------------------------------------------------------------------

CREATE TABLE library_items (
    id         INTEGER PRIMARY KEY,
    subject_id INTEGER          REFERENCES subjects(id) ON DELETE SET NULL,
    kind       TEXT    NOT NULL CHECK (kind IN
                    ('notes', 'practice_question', 'weekly_review', 'answer', 'cards')),
    title      TEXT    NOT NULL,
    -- What was asked. Kept so an item is reproducible and you can see why it
    -- says what it says.
    prompt     TEXT,
    body       TEXT    NOT NULL,
    -- Which model wrote it. Output quality varies between models and between
    -- versions of the same alias, so an undated, unattributed note is hard to
    -- trust later.
    model      TEXT,
    pinned     INTEGER NOT NULL DEFAULT 0 CHECK (pinned IN (0, 1)),
    created_at TEXT    NOT NULL
);
CREATE INDEX idx_library_created ON library_items(created_at DESC);
CREATE INDEX idx_library_subject ON library_items(subject_id, kind);
