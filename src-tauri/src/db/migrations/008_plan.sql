-- The plan: what you meant to do, and when.
--
-- Retain could tell you what was due and when you were free, but it had nowhere
-- to hold an intention — "revise enzyme kinetics on Thursday". Without that
-- there is nothing for a missed evening to slip *from*, so a bad Tuesday just
-- disappeared instead of moving Wednesday.
CREATE TABLE plan_items (
  id INTEGER PRIMARY KEY,
  subject_id INTEGER REFERENCES subjects(id) ON DELETE CASCADE,
  title TEXT NOT NULL,
  detail TEXT,

  -- The day it currently sits on.
  planned_on TEXT NOT NULL,
  -- Where it started. Never rewritten, so "this has been slipping since the 3rd"
  -- is answerable — a plan that quietly forgets its own history can't tell you
  -- the one thing you most need to hear.
  first_planned_on TEXT NOT NULL,

  est_minutes INTEGER NOT NULL DEFAULT 30,

  -- A hard date this cannot move past: a SAC, an exam, a due assignment.
  -- Rollover refuses to schedule work after the thing it was for.
  due_on TEXT,

  status TEXT NOT NULL DEFAULT 'planned'
    CHECK (status IN ('planned', 'done', 'skipped')),
  -- How many times rollover has moved it. Surfaced, not hidden: three moves
  -- means the plan is wrong, not that you are lazy.
  moves INTEGER NOT NULL DEFAULT 0,

  source TEXT NOT NULL DEFAULT 'manual'
    CHECK (source IN ('manual', 'ai', 'assessment')),

  created_at TEXT NOT NULL,
  done_at TEXT
);

CREATE INDEX idx_plan_day ON plan_items(planned_on, status);
CREATE INDEX idx_plan_subject ON plan_items(subject_id);
