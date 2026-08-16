-- ---------------------------------------------------------------------------
-- Notes you write, as blocks.
--
-- The one thing Retain still made you leave for. Everything else it holds is
-- something it generated or you uploaded; there was nowhere to write down what
-- the teacher said in fourth period.
--
-- Stored as blocks rather than one Markdown string. A single text column is
-- simpler and wrong for what this has to do next: a checkbox has state, an
-- image has bytes, and a block has to be reorderable without rewriting the
-- document around it. Markdown can be *rendered* from these at any time —
-- that's the export path — but it can't be the storage.
-- ---------------------------------------------------------------------------

CREATE TABLE notes (
    id         INTEGER PRIMARY KEY,
    subject_id INTEGER          REFERENCES subjects(id) ON DELETE SET NULL,
    topic_id   INTEGER          REFERENCES topics(id)   ON DELETE SET NULL,
    title      TEXT    NOT NULL DEFAULT 'Untitled',
    -- The class this came out of, when it was written from the week grid.
    -- Lets a note be filed under "what did I do in Chemistry on the 14th".
    on_date    TEXT,
    archived   INTEGER NOT NULL DEFAULT 0 CHECK (archived IN (0, 1)),
    created_at TEXT    NOT NULL,
    updated_at TEXT    NOT NULL
);
CREATE INDEX idx_notes_subject ON notes(subject_id, archived);
CREATE INDEX idx_notes_date    ON notes(on_date);

CREATE TABLE note_blocks (
    id       INTEGER PRIMARY KEY,
    note_id  INTEGER NOT NULL REFERENCES notes(id) ON DELETE CASCADE,

    -- Dense and contiguous: 0, 1, 2… Renumbered on every structural change.
    -- Fractional indices avoid the renumber but drift into float precision
    -- after enough inserts between the same two blocks, and a note is dozens of
    -- rows, not millions — the rewrite is cheaper than the class of bug.
    position INTEGER NOT NULL,

    kind     TEXT    NOT NULL CHECK (kind IN (
                 'paragraph', 'h1', 'h2', 'h3',
                 'bullet', 'numbered', 'todo',
                 'quote', 'code', 'divider', 'image')),
    text     TEXT    NOT NULL DEFAULT '',
    -- Only meaningful for 'todo'.
    checked  INTEGER NOT NULL DEFAULT 0 CHECK (checked IN (0, 1)),
    -- A `data:image/...;base64,` URL for 'image'. Held in the row rather than
    -- on disk so a note stays intact when it's exported or the file moves.
    image    TEXT
);
CREATE UNIQUE INDEX idx_blocks_order ON note_blocks(note_id, position);
