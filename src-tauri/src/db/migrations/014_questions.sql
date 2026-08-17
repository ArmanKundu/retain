-- ---------------------------------------------------------------------------
-- Past exam questions, individually.
--
-- A thousand exam papers in the library and no way to say "show me every
-- calculus question in Specialist". Full-text search over a whole paper gives
-- you the paper; what you want is the question.
--
-- Segmented from the text that is already stored, so this costs no new import
-- and no second copy of anything — a question is a span of a resource, and
-- deleting the resource takes its questions with it.
-- ---------------------------------------------------------------------------

CREATE TABLE questions (
    id          INTEGER PRIMARY KEY,
    resource_id INTEGER NOT NULL REFERENCES resources(id) ON DELETE CASCADE,
    -- Denormalised from the resource so a search can filter by subject without
    -- joining, and survive the resource's subject changing to NULL.
    subject_id  INTEGER          REFERENCES subjects(id) ON DELETE SET NULL,

    -- "Question 3" as printed. Kept verbatim rather than parsed into parts,
    -- because papers number themselves inconsistently and the printed label is
    -- what you'd search for.
    label       TEXT    NOT NULL,
    number      INTEGER NOT NULL,
    -- Position within the paper, for ordering.
    ordinal     INTEGER NOT NULL,
    text        TEXT    NOT NULL,
    -- Rough size, so a one-mark multiple choice can be told from a ten-mark
    -- extended response without reading it.
    words       INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_questions_resource ON questions(resource_id, ordinal);
CREATE INDEX idx_questions_subject  ON questions(subject_id);

-- External-content FTS, same arrangement as `resource_chunks`: the index holds
-- no second copy of the text, and the triggers below are what keep it honest.
-- Without them a deleted question stays searchable forever.
CREATE VIRTUAL TABLE questions_fts USING fts5(
    text,
    content = 'questions',
    content_rowid = 'id',
    tokenize = 'porter unicode61'
);

CREATE TRIGGER questions_ai AFTER INSERT ON questions BEGIN
    INSERT INTO questions_fts(rowid, text) VALUES (new.id, new.text);
END;
CREATE TRIGGER questions_ad AFTER DELETE ON questions BEGIN
    INSERT INTO questions_fts(questions_fts, rowid, text) VALUES ('delete', old.id, old.text);
END;
CREATE TRIGGER questions_au AFTER UPDATE ON questions BEGIN
    INSERT INTO questions_fts(questions_fts, rowid, text) VALUES ('delete', old.id, old.text);
    INSERT INTO questions_fts(rowid, text) VALUES (new.id, new.text);
END;

-- Tags. A table rather than a text column so filtering and counting are index
-- lookups, and so a rename is one statement.
CREATE TABLE question_tags (
    question_id INTEGER NOT NULL REFERENCES questions(id) ON DELETE CASCADE,
    tag         TEXT    NOT NULL,
    -- Whether Retain suggested it or you typed it. Suggested tags come from
    -- your own topic names appearing in the question; they are a starting
    -- point, and being able to tell them apart is what makes them safe to
    -- offer at all.
    source      TEXT    NOT NULL DEFAULT 'manual' CHECK (source IN ('manual', 'auto')),
    PRIMARY KEY (question_id, tag)
);
CREATE INDEX idx_question_tags_tag ON question_tags(tag);
