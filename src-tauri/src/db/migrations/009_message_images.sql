-- Images on an assistant message.
--
-- Attachments were text only: a PDF was extracted to prose and sent as words.
-- A screenshot has no prose to extract — the image *is* the question — so it is
-- stored as a data URL and passed to the model's vision input.
--
-- Held in the row rather than on disk because a conversation you can reopen in
-- February and still see the diagram in is the whole reason to keep it.
ALTER TABLE message_attachments ADD COLUMN image_data_url TEXT;
