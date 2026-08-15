-- ---------------------------------------------------------------------------
-- A real taxonomy for uploaded material.
--
-- Four kinds wasn't enough to file a year of VCE by. The distinction that
-- matters is not "what format is this" but "what authority does it carry":
-- a study design says what is examinable, an examiner's report says how it was
-- marked, your own notes say what you understood at the time. The assistant
-- weights them differently, so they have to be distinguishable.
--
-- SQLite can't widen a CHECK constraint in place, so the table is rebuilt.
-- Everything is copied first and the FTS index is regenerated from the copy —
-- losing a year of indexed material to a schema change would be unforgivable.
-- ---------------------------------------------------------------------------

CREATE TABLE resources_new (
    id          INTEGER PRIMARY KEY,
    subject_id  INTEGER          REFERENCES subjects(id) ON DELETE SET NULL,
    title       TEXT    NOT NULL,
    kind        TEXT    NOT NULL CHECK (kind IN (
                    'study_design',     -- VCAA: what is examinable
                    'past_paper',       -- how it gets asked
                    'exam_solution',    -- marking scheme / examiner's report
                    'school_notes',     -- from your teacher
                    'personal_notes',   -- your own
                    'textbook',
                    'other')),
    source      TEXT,
    content     TEXT    NOT NULL,
    word_count  INTEGER NOT NULL DEFAULT 0,
    added_at    TEXT    NOT NULL,
    origin_path TEXT
);

INSERT INTO resources_new (id, subject_id, title, kind, source, content, word_count, added_at, origin_path)
    SELECT id, subject_id, title,
           -- The old 'notes' didn't say whose. Existing rows become school
           -- notes, which is the commoner case and the safer default: it is
           -- treated as more authoritative than personal notes, never less.
           CASE kind WHEN 'notes' THEN 'school_notes' ELSE kind END,
           source, content, word_count, added_at, origin_path
      FROM resources;

DROP TABLE resources;
ALTER TABLE resources_new RENAME TO resources;

CREATE INDEX idx_resources_subject ON resources(subject_id, kind);
CREATE INDEX idx_resources_origin  ON resources(origin_path);

-- The chunks referenced the dropped table, so the FTS index is rebuilt from
-- what survived rather than trusted.
INSERT INTO resource_chunks_fts(resource_chunks_fts) VALUES ('rebuild');

-- ---------------------------------------------------------------------------
-- Time blocks: when you can't study.
--
-- Retain knew what you had to do and never knew when you could do it. A week
-- with tuition on Tuesday and a shift on Saturday is a different week from an
-- empty one, and any advice that ignores that is advice you'll ignore back.
--
-- Blocks are either weekly (a recurring commitment) or dated (a one-off). One
-- table, because a block is the same thing either way — only its recurrence
-- differs, and splitting it would double every query.
-- ---------------------------------------------------------------------------

CREATE TABLE time_blocks (
    id          INTEGER PRIMARY KEY,
    title       TEXT    NOT NULL,
    kind        TEXT    NOT NULL CHECK (kind IN (
                    'class',     -- school
                    'tuition',
                    'work',
                    'commute',
                    'exercise',
                    'family',
                    'rest',      -- deliberately protected, not "spare"
                    'other')),
    -- Exactly one of these is set. `weekday` is 0=Monday, matching chrono's
    -- num_days_from_monday, which is what every other date calculation here
    -- already uses.
    weekday     INTEGER          CHECK (weekday BETWEEN 0 AND 6),
    on_date     TEXT,
    -- Minutes from local midnight. Integers rather than times because every
    -- operation on them is arithmetic — overlap, duration, sorting.
    start_min   INTEGER NOT NULL CHECK (start_min BETWEEN 0 AND 1439),
    end_min     INTEGER NOT NULL CHECK (end_min BETWEEN 1 AND 1440),
    -- Whether study can happen here. A class you can revise through is
    -- different from a shift you can't, and only you know which is which.
    available   INTEGER NOT NULL DEFAULT 0 CHECK (available IN (0, 1)),
    subject_id  INTEGER          REFERENCES subjects(id) ON DELETE SET NULL,
    note        TEXT,
    created_at  TEXT    NOT NULL,

    CHECK (end_min > start_min),
    -- A block is weekly or dated, never both and never neither.
    CHECK ((weekday IS NULL) != (on_date IS NULL))
);
CREATE INDEX idx_blocks_weekday ON time_blocks(weekday);
CREATE INDEX idx_blocks_date    ON time_blocks(on_date);
