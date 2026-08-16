-- ---------------------------------------------------------------------------
-- Units, and school trial exams.
--
-- Two things the taxonomy couldn't say.
--
-- **Which unit.** "Unit 3 notes" and "Unit 4 notes" are different material, and
-- an assistant answering a Unit 4 question out of your Unit 3 notes is worse
-- than one that says it doesn't know. But the unit dimension does not apply
-- evenly: a study design covers the whole 3&4 sequence, a VCAA exam examines
-- both units at once, and a textbook spans the year. Only the things you
-- actually file per unit — your notes, your school's notes, your trial tests —
-- carry a unit. Everything else is NULL, meaning "both", and that is a real
-- distinction rather than missing data.
--
-- **Trial tests.** A school trial exam is not a VCAA paper and shouldn't be
-- weighted like one: it's someone's best guess at what VCAA will ask. It
-- deserves its own kind, ranked below the real thing and above notes.
--
-- Rebuilding `resources` again to widen the kind CHECK. The chunks are copied
-- aside first — see migration 005 for what happens when they aren't.
-- ---------------------------------------------------------------------------

CREATE TEMP TABLE chunks_backup AS
    SELECT id, resource_id, ordinal, content FROM resource_chunks;

CREATE TABLE resources_new (
    id          INTEGER PRIMARY KEY,
    subject_id  INTEGER          REFERENCES subjects(id) ON DELETE SET NULL,
    title       TEXT    NOT NULL,
    kind        TEXT    NOT NULL CHECK (kind IN (
                    'study_design',     -- VCAA: what is examinable
                    'past_paper',       -- VCAA: how it gets asked
                    'exam_solution',    -- marking scheme / examiner's report
                    'trial_test',       -- a school's practice exam
                    'textbook',
                    'school_notes',     -- from your teacher
                    'personal_notes',   -- your own
                    'other')),
    -- 3, 4, or NULL for material that spans the sequence. Units 1 and 2 are
    -- stored the same way for a 1/2 subject.
    unit        INTEGER          CHECK (unit IS NULL OR unit BETWEEN 1 AND 4),
    source      TEXT,
    content     TEXT    NOT NULL,
    word_count  INTEGER NOT NULL DEFAULT 0,
    added_at    TEXT    NOT NULL,
    origin_path TEXT
);

INSERT INTO resources_new
       (id, subject_id, title, kind, unit, source, content, word_count, added_at, origin_path)
    SELECT id, subject_id, title, kind, NULL, source, content, word_count, added_at, origin_path
      FROM resources;

DROP TABLE resources;
ALTER TABLE resources_new RENAME TO resources;

CREATE INDEX idx_resources_subject ON resources(subject_id, kind);
CREATE INDEX idx_resources_origin  ON resources(origin_path);
CREATE INDEX idx_resources_unit    ON resources(subject_id, unit);

-- Put the chunks back, keeping their ids because the FTS index keys off them.
INSERT INTO resource_chunks (id, resource_id, ordinal, content)
    SELECT b.id, b.resource_id, b.ordinal, b.content
      FROM chunks_backup b
     WHERE EXISTS (SELECT 1 FROM resources r WHERE r.id = b.resource_id);

DROP TABLE chunks_backup;

INSERT INTO resource_chunks_fts(resource_chunks_fts) VALUES ('rebuild');
