-- ---------------------------------------------------------------------------
-- Screenshots and files on a capture.
--
-- The point of ⌘⇧Space in class is that you have four seconds. Typing "the
-- thing on the board about enzymes" is worse than a photograph of the board,
-- and by the evening the note alone often isn't enough to reconstruct what you
-- meant.
--
-- Images are stored as bytes; documents as extracted text. Different columns
-- rather than one blob, because they're used differently: an image is shown
-- back to you during triage, text is searchable and can ground an answer.
-- ---------------------------------------------------------------------------

CREATE TABLE capture_attachments (
    id         INTEGER PRIMARY KEY,
    capture_id INTEGER NOT NULL REFERENCES captures(id) ON DELETE CASCADE,
    name       TEXT    NOT NULL,
    -- 'image' or 'text'. Exactly one of the two payload columns is set.
    kind       TEXT    NOT NULL CHECK (kind IN ('image', 'text')),
    image      BLOB,
    text       TEXT,
    created_at TEXT    NOT NULL,

    CHECK ((kind = 'image') = (image IS NOT NULL)),
    CHECK ((kind = 'text')  = (text  IS NOT NULL))
);
CREATE INDEX idx_capture_attachments ON capture_attachments(capture_id);
