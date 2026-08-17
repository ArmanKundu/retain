-- ---------------------------------------------------------------------------
-- Sticky notes on the desktop.
--
-- A sticky is a note, not a second kind of object. Same table, same blocks,
-- same editor, same Markdown — the only difference is that it is also drawn in
-- a small always-on-top window, and remembers where you put it.
--
-- The alternative was a `stickies` table with a plain text column. That reads
-- simpler and immediately costs you two editors, two sets of shortcuts, and a
-- note you can't promote into a real one when it turns out to matter.
-- ---------------------------------------------------------------------------

-- Where the window sits. NULL means "not a sticky" — it has never been opened
-- on the desktop.
ALTER TABLE notes ADD COLUMN sticky_x REAL;
ALTER TABLE notes ADD COLUMN sticky_y REAL;
ALTER TABLE notes ADD COLUMN sticky_w REAL;
ALTER TABLE notes ADD COLUMN sticky_h REAL;

-- Whether it should reopen on the desktop next launch. Separate from the
-- geometry so closing a sticky keeps its position for when you open it again.
ALTER TABLE notes ADD COLUMN sticky_open INTEGER NOT NULL DEFAULT 0
    CHECK (sticky_open IN (0, 1));

-- Paper colour. Not the subject's colour: a sticky is sorted by where it is on
-- your screen, and you need to be able to say "the yellow one".
ALTER TABLE notes ADD COLUMN sticky_colour TEXT NOT NULL DEFAULT 'amber';

CREATE INDEX idx_notes_sticky ON notes(sticky_open) WHERE sticky_open = 1;
