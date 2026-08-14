-- ---------------------------------------------------------------------------
-- The assistant.
--
-- Conversations are kept in full — questions, answers, and which passages of
-- your own material each answer was built from. An assistant whose answers
-- vanish when you close the window is a worse version of a search box.
-- ---------------------------------------------------------------------------

CREATE TABLE conversations (
    id         INTEGER PRIMARY KEY,
    subject_id INTEGER          REFERENCES subjects(id) ON DELETE SET NULL,
    title      TEXT    NOT NULL,
    -- How the assistant is allowed to answer.
    --
    --   'strict' — only from material you supplied. If your notes don't cover
    --              it, it says so instead of filling the gap from memory. This
    --              is the default, because a confident answer you can't trace
    --              is the failure mode that makes a study tool dangerous.
    --   'open'   — your material first, then its own knowledge, labelled.
    grounding  TEXT    NOT NULL DEFAULT 'strict' CHECK (grounding IN ('strict', 'open')),
    created_at TEXT    NOT NULL,
    updated_at TEXT    NOT NULL
);
CREATE INDEX idx_conversations_updated ON conversations(updated_at DESC);

CREATE TABLE messages (
    id              INTEGER PRIMARY KEY,
    conversation_id INTEGER NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    role            TEXT    NOT NULL CHECK (role IN ('user', 'assistant')),
    body            TEXT    NOT NULL,
    -- The excerpts that grounded this answer, as JSON. Stored rather than
    -- recomputed: retrieval depends on what was in the library at the time, so
    -- re-running it later would show citations the answer never actually used.
    sources         TEXT,
    model           TEXT,
    created_at      TEXT    NOT NULL
);
CREATE INDEX idx_messages_conversation ON messages(conversation_id, id);

-- A file attached to one message. Kept as text, like a resource, but scoped to
-- the conversation rather than added to the searchable library — attaching a
-- worksheet to ask one question shouldn't permanently change what every future
-- answer is grounded in.
CREATE TABLE message_attachments (
    id         INTEGER PRIMARY KEY,
    message_id INTEGER NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    name       TEXT    NOT NULL,
    content    TEXT    NOT NULL,
    words      INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_attachments_message ON message_attachments(message_id);

-- Where a subject's files live on disk, so a folder can be re-synced rather
-- than re-picked every time.
ALTER TABLE subjects ADD COLUMN folder_path TEXT;

-- Which folder a resource came from, so a re-sync can tell what it already has.
ALTER TABLE resources ADD COLUMN origin_path TEXT;
CREATE INDEX idx_resources_origin ON resources(origin_path);
