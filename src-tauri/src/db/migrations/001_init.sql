-- Checkpoint 1 schema.
--
-- Conventions used throughout:
--   * Instants are stored as RFC 3339 strings in UTC ("2026-08-12T04:11:09Z").
--   * `local_date` columns are 'YYYY-MM-DD' in the user's LOCAL timezone. Day
--     bucketing (the contribution grid, the streak) must follow local midnight,
--     not UTC midnight, or a 9pm Melbourne session lands on the wrong day.
--   * Booleans are INTEGER 0/1 with a CHECK, since SQLite has no bool type.

CREATE TABLE app_settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE subjects (
    id                  INTEGER PRIMARY KEY,
    name                TEXT    NOT NULL UNIQUE,
    colour              TEXT    NOT NULL,
    -- Unit level drives behaviour: 3/4 subjects get exam countdowns, the VCAA
    -- topic tree and revision scheduling; 1/2 subjects get timers and notes only.
    unit_level          TEXT    NOT NULL CHECK (unit_level IN ('1_2', '3_4')),
    -- Type flag drives which error-log categories and card templates are offered.
    subject_type        TEXT    NOT NULL CHECK (subject_type IN ('science', 'maths', 'english', 'humanities')),
    weekly_goal_minutes INTEGER,          -- NULL = no goal set for this subject
    sort_order          INTEGER NOT NULL DEFAULT 0,
    archived            INTEGER NOT NULL DEFAULT 0 CHECK (archived IN (0, 1)),
    created_at          TEXT    NOT NULL
);

-- The topic tree. Checkpoint 3 populates Biology 3/4 from a JSON file; for now
-- it exists so sessions can carry an optional topic without a later migration
-- rewriting the sessions table.
CREATE TABLE topics (
    id         INTEGER PRIMARY KEY,
    subject_id INTEGER NOT NULL REFERENCES subjects(id) ON DELETE CASCADE,
    parent_id  INTEGER          REFERENCES topics(id)   ON DELETE CASCADE,
    name       TEXT    NOT NULL,
    kind       TEXT,                      -- 'unit' | 'aos' | 'dot_point' | NULL for free-form
    sort_order INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_topics_subject ON topics(subject_id);

CREATE TABLE sessions (
    id               INTEGER PRIMARY KEY,
    subject_id       INTEGER NOT NULL REFERENCES subjects(id) ON DELETE CASCADE,
    topic_id         INTEGER          REFERENCES topics(id)   ON DELETE SET NULL,
    mode             TEXT    NOT NULL CHECK (mode IN ('stopwatch', 'pomodoro')),
    started_at       TEXT    NOT NULL,
    ended_at         TEXT,                -- NULL while the session is running
    local_date       TEXT    NOT NULL,
    -- Wall clock from start to stop.
    elapsed_seconds  INTEGER NOT NULL DEFAULT 0,
    -- Elapsed MINUS manual pauses MINUS idle auto-pauses MINUS pomodoro breaks.
    -- This is the only duration the streak is allowed to look at. See
    -- docs/streak-rule.md for why the distinction matters.
    active_seconds   INTEGER NOT NULL DEFAULT 0,
    pause_count      INTEGER NOT NULL DEFAULT 0,
    idle_pause_count INTEGER NOT NULL DEFAULT 0,
    note             TEXT
);
CREATE INDEX idx_sessions_local_date ON sessions(local_date);
CREATE INDEX idx_sessions_subject    ON sessions(subject_id);

-- One row per pause interval, so active time is derived from recorded facts
-- rather than a running counter that can drift.
CREATE TABLE session_pauses (
    id         INTEGER PRIMARY KEY,
    session_id INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    paused_at  TEXT    NOT NULL,
    resumed_at TEXT,                      -- NULL while still paused
    reason     TEXT    NOT NULL CHECK (reason IN ('manual', 'idle', 'break'))
);
CREATE INDEX idx_pauses_session ON session_pauses(session_id);

-- Written ONLY by the act of rating an item that was actually presented.
-- Checkpoint 1 never inserts here; it exists now so the streak's "reviews
-- cleared" branch is structurally honest from day one rather than being
-- retrofitted onto a table that was designed for something else.
CREATE TABLE review_log (
    id           INTEGER PRIMARY KEY,
    item_type    TEXT    NOT NULL CHECK (item_type IN ('card', 'error_reattempt')),
    item_id      INTEGER NOT NULL,
    subject_id   INTEGER          REFERENCES subjects(id) ON DELETE SET NULL,
    due_on       TEXT    NOT NULL,        -- local date the item was due
    presented_at TEXT    NOT NULL,        -- when it was shown to the user
    rated_at     TEXT    NOT NULL,        -- when the rating was committed
    duration_ms  INTEGER NOT NULL,        -- rated_at - presented_at, kept for auditability
    rating       INTEGER NOT NULL CHECK (rating BETWEEN 1 AND 4),
    local_date   TEXT    NOT NULL
);
CREATE INDEX idx_review_log_local_date ON review_log(local_date);

-- Weekdays the user has nominated as rest days. 0 = Monday .. 6 = Sunday
-- (chrono's num_days_from_monday). A non-qualifying rest day neither breaks the
-- run nor consumes a freeze.
CREATE TABLE rest_days (
    weekday INTEGER PRIMARY KEY CHECK (weekday BETWEEN 0 AND 6)
);

-- Freezes as a grant/consume ledger rather than a counter, so the history stays
-- inspectable and cannot silently drift out of sync with reality.
CREATE TABLE streak_freezes (
    id          INTEGER PRIMARY KEY,
    granted_on  TEXT NOT NULL,
    consumed_on TEXT                      -- NULL = still available
);

-- Start with the full two. The brief says "2 freezes available at once", which
-- means you HAVE two, not that you can eventually earn two — a new user being
-- told they have zero safety net until they have already studied for a fortnight
-- gets the incentive exactly backwards.
INSERT INTO streak_freezes (granted_on) VALUES (date('now')), (date('now'));
