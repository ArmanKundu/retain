-- Checkpoint 2 and 3 schema: quick capture, tasks, flashcards, the error log,
-- assessments, retrospective revision, calendar, notifications, practice exams.
--
-- Same conventions as 001: RFC 3339 UTC for instants, 'YYYY-MM-DD' LOCAL dates
-- for anything bucketed by day, INTEGER 0/1 for booleans.

-- ---------------------------------------------------------------------------
-- Quick capture and its inbox
-- ---------------------------------------------------------------------------

CREATE TABLE captures (
    id                   INTEGER PRIMARY KEY,
    raw_text             TEXT    NOT NULL,
    created_at           TEXT    NOT NULL,
    local_date           TEXT    NOT NULL,
    -- Results of natural-language parsing, offered as suggestions during triage.
    -- Never applied automatically: a wrong guess silently filed in the wrong
    -- place is worse than no guess.
    suggested_subject_id INTEGER          REFERENCES subjects(id) ON DELETE SET NULL,
    suggested_due_on     TEXT,
    suggested_title      TEXT,
    triaged_at           TEXT,
    triaged_to           TEXT CHECK (triaged_to IN ('task', 'card', 'error_entry', 'discarded'))
);
CREATE INDEX idx_captures_untriaged ON captures(triaged_at) WHERE triaged_at IS NULL;

CREATE TABLE tasks (
    id                INTEGER PRIMARY KEY,
    title             TEXT    NOT NULL,
    subject_id        INTEGER          REFERENCES subjects(id) ON DELETE SET NULL,
    due_on            TEXT,
    done_at           TEXT,
    created_at        TEXT    NOT NULL,
    source_capture_id INTEGER          REFERENCES captures(id) ON DELETE SET NULL
);
CREATE INDEX idx_tasks_due ON tasks(due_on) WHERE done_at IS NULL;

-- ---------------------------------------------------------------------------
-- Flashcards (FSRS-6)
-- ---------------------------------------------------------------------------

CREATE TABLE cards (
    id            INTEGER PRIMARY KEY,
    subject_id    INTEGER NOT NULL REFERENCES subjects(id) ON DELETE CASCADE,
    topic_id      INTEGER          REFERENCES topics(id)   ON DELETE SET NULL,

    -- 'quote' is the English-subject card type: quote → source/context → theme.
    note_type     TEXT    NOT NULL CHECK (note_type IN ('basic', 'cloze', 'quote')),
    front         TEXT    NOT NULL,
    back          TEXT    NOT NULL,
    extra         TEXT,                    -- quote: theme tag. cloze: the full source text.
    cloze_index   INTEGER,                 -- which {{cN::}} this card renders; NULL for basic/quote
    tags          TEXT,                    -- space-separated, Anki's format

    -- FSRS state. `stability` and `difficulty` are NULL until the first review:
    -- a new card has no memory state, and 0.0 would be a lie the scheduler acts on.
    state         TEXT    NOT NULL CHECK (state IN ('new', 'learning', 'review', 'relearning')),
    stability     REAL,
    difficulty    REAL,
    -- Instant the card is next due. Learning steps are intraday, so this needs
    -- to be a full timestamp, not a date.
    due_at        TEXT,
    -- The same moment as a LOCAL date, so day-bucketed queries ("due today")
    -- don't have to do timezone arithmetic in SQL.
    due_on        TEXT,
    last_review_at TEXT,
    -- The Retain day this card stopped being new. This is what the daily
    -- new-card cap counts: a card introduced today consumes today's allowance
    -- permanently, so the cap can't be dodged by answering Again repeatedly.
    introduced_on TEXT,
    reps          INTEGER NOT NULL DEFAULT 0,
    lapses        INTEGER NOT NULL DEFAULT 0,
    learning_step INTEGER NOT NULL DEFAULT 0,
    suspended     INTEGER NOT NULL DEFAULT 0 CHECK (suspended IN (0, 1)),

    -- Set at import time, used to skip duplicates on re-import.
    content_hash  TEXT    NOT NULL,
    created_at    TEXT    NOT NULL
);
CREATE INDEX idx_cards_due     ON cards(due_on)  WHERE suspended = 0;
CREATE INDEX idx_cards_state   ON cards(state)   WHERE suspended = 0;
CREATE INDEX idx_cards_subject ON cards(subject_id);
CREATE UNIQUE INDEX idx_cards_dedupe ON cards(subject_id, content_hash);

-- ---------------------------------------------------------------------------
-- Error log
-- ---------------------------------------------------------------------------

CREATE TABLE error_entries (
    id              INTEGER PRIMARY KEY,
    subject_id      INTEGER NOT NULL REFERENCES subjects(id) ON DELETE CASCADE,
    topic_id        INTEGER          REFERENCES topics(id)   ON DELETE SET NULL,
    logged_on       TEXT    NOT NULL,
    source          TEXT,                  -- paper / SAC name + question number
    command_word    TEXT,                  -- 3/4 subjects; VCAA glossary term
    question_text   TEXT,
    question_image  BLOB,                  -- pasted screenshot
    my_answer       TEXT,                  -- what was actually written
    correct_answer  TEXT,                  -- mark-scheme point
    category        TEXT    NOT NULL,      -- from the per-subject-type pick list
    fix             TEXT,                  -- one-sentence takeaway
    marks_lost      INTEGER,
    marks_available INTEGER,
    revisit_on      TEXT,                  -- local date the blind re-attempt is due
    fixed_at        TEXT,                  -- set ONLY by a correct blind re-attempt
    created_at      TEXT    NOT NULL
);
CREATE INDEX idx_errors_subject ON error_entries(subject_id);
CREATE INDEX idx_errors_revisit ON error_entries(revisit_on) WHERE fixed_at IS NULL;
CREATE INDEX idx_errors_category ON error_entries(category);

-- A blind re-attempt at a logged error.
--
-- The whole point of the error log is that re-reading the correct answer
-- produces an illusion of competence. So the ordering here is a constraint, not
-- a convention: an answer must be COMMITTED before it can be REVEALED, and only
-- a re-attempt that was committed blind and then self-marked correct is allowed
-- to mark the parent entry fixed.
CREATE TABLE error_reattempts (
    id              INTEGER PRIMARY KEY,
    error_entry_id  INTEGER NOT NULL REFERENCES error_entries(id) ON DELETE CASCADE,
    presented_at    TEXT    NOT NULL,      -- question shown, answer field empty
    committed_at    TEXT,                  -- blind answer locked in
    blind_answer    TEXT,                  -- written WITHOUT having seen the answer
    revealed_at     TEXT,                  -- model answer shown; never before committed_at
    self_assessment TEXT CHECK (self_assessment IN ('correct', 'partial', 'incorrect')),
    marks_awarded   INTEGER,
    local_date      TEXT    NOT NULL,

    -- Enforced in the database as well as in code, because this is the one
    -- invariant the whole feature rests on.
    CHECK (revealed_at IS NULL OR committed_at IS NOT NULL),
    CHECK (self_assessment IS NULL OR committed_at IS NOT NULL)
);
CREATE INDEX idx_reattempts_entry ON error_reattempts(error_entry_id);

-- ---------------------------------------------------------------------------
-- Assessments and retrospective revision
-- ---------------------------------------------------------------------------

CREATE TABLE assessments (
    id         INTEGER PRIMARY KEY,
    subject_id INTEGER NOT NULL REFERENCES subjects(id) ON DELETE CASCADE,
    name       TEXT    NOT NULL,
    kind       TEXT    NOT NULL CHECK (kind IN ('sac', 'exam', 'other')),
    due_on     TEXT    NOT NULL,
    source     TEXT    NOT NULL DEFAULT 'manual' CHECK (source IN ('manual', 'compass')),
    -- Set for Compass-imported rows so a refresh updates rather than duplicates.
    external_uid TEXT,
    created_at TEXT    NOT NULL
);
CREATE INDEX idx_assessments_due ON assessments(due_on);
CREATE UNIQUE INDEX idx_assessments_external ON assessments(external_uid)
    WHERE external_uid IS NOT NULL;

CREATE TABLE assessment_topics (
    assessment_id INTEGER NOT NULL REFERENCES assessments(id) ON DELETE CASCADE,
    topic_id      INTEGER NOT NULL REFERENCES topics(id)      ON DELETE CASCADE,
    PRIMARY KEY (assessment_id, topic_id)
);

-- One row per time you tested yourself on a topic.
--
-- This is the retrospective timetable's entire data model: what you reviewed,
-- when, and how confident you were afterwards. There is deliberately no table
-- of "topics assigned to future dates" — a prospective timetable breaks the
-- first time life intervenes, and the brief rules it out.
CREATE TABLE topic_reviews (
    id          INTEGER PRIMARY KEY,
    topic_id    INTEGER NOT NULL REFERENCES topics(id) ON DELETE CASCADE,
    reviewed_at TEXT    NOT NULL,
    local_date  TEXT    NOT NULL,
    confidence  INTEGER NOT NULL CHECK (confidence BETWEEN 1 AND 5),
    note        TEXT
);
CREATE INDEX idx_topic_reviews_topic ON topic_reviews(topic_id, reviewed_at DESC);

-- ---------------------------------------------------------------------------
-- Compass calendar (ICS subscription only — never credentials, never scraping)
-- ---------------------------------------------------------------------------

CREATE TABLE calendar_events (
    id            INTEGER PRIMARY KEY,
    uid           TEXT    NOT NULL,        -- ICS UID
    -- Set for one instance of a recurring series, so an expanded occurrence
    -- has a stable identity across refreshes.
    recurrence_id TEXT,
    summary       TEXT    NOT NULL,
    description   TEXT,
    starts_at     TEXT    NOT NULL,        -- RFC 3339 UTC
    ends_at       TEXT,
    all_day       INTEGER NOT NULL DEFAULT 0 CHECK (all_day IN (0, 1)),
    local_date    TEXT    NOT NULL,
    fetched_at    TEXT    NOT NULL
);
CREATE UNIQUE INDEX idx_calendar_identity ON calendar_events(uid, COALESCE(recurrence_id, ''));
CREATE INDEX idx_calendar_date ON calendar_events(local_date);

-- ---------------------------------------------------------------------------
-- Notifications
-- ---------------------------------------------------------------------------

-- Every notification actually delivered. Used to enforce the daily cap and to
-- stop the same thing being said twice.
CREATE TABLE notification_log (
    id         INTEGER PRIMARY KEY,
    category   TEXT NOT NULL CHECK (category IN ('reviews', 'assessments', 'topic_decay', 'streak')),
    sent_at    TEXT NOT NULL,
    local_date TEXT NOT NULL,
    title      TEXT NOT NULL,
    body       TEXT NOT NULL,
    -- Identifies "this exact thing" so it isn't repeated within its cadence
    -- window, e.g. 'assessment:12:14d'.
    dedupe_key TEXT NOT NULL
);
CREATE INDEX idx_notifications_date ON notification_log(local_date);
CREATE INDEX idx_notifications_dedupe ON notification_log(dedupe_key, sent_at DESC);

-- ---------------------------------------------------------------------------
-- Practice exams (Biology 3/4)
-- ---------------------------------------------------------------------------

CREATE TABLE practice_exams (
    id              INTEGER PRIMARY KEY,
    subject_id      INTEGER NOT NULL REFERENCES subjects(id) ON DELETE CASCADE,
    name            TEXT    NOT NULL,
    taken_on        TEXT    NOT NULL,
    -- VCAA Biology structure: Section A is 40 multiple choice for 40 marks,
    -- Section B is short and extended response for 80 marks.
    section_a_score INTEGER,
    section_a_max   INTEGER NOT NULL DEFAULT 40,
    section_b_score INTEGER,
    section_b_max   INTEGER NOT NULL DEFAULT 80,
    reading_seconds INTEGER,
    writing_seconds INTEGER,
    created_at      TEXT    NOT NULL
);
CREATE INDEX idx_practice_exams_subject ON practice_exams(subject_id, taken_on DESC);

CREATE TABLE practice_exam_aos (
    id               INTEGER PRIMARY KEY,
    practice_exam_id INTEGER NOT NULL REFERENCES practice_exams(id) ON DELETE CASCADE,
    topic_id         INTEGER          REFERENCES topics(id) ON DELETE SET NULL,
    aos_label        TEXT    NOT NULL,
    marks_scored     INTEGER NOT NULL,
    marks_available  INTEGER NOT NULL
);
CREATE INDEX idx_practice_aos_exam ON practice_exam_aos(practice_exam_id);
