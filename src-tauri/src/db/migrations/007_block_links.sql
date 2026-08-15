-- A link on a time block.
--
-- Tuition on Zoom, a class on Teams, a study group on Meet. The link lives with
-- the commitment rather than in a browser tab you have to go and find, which is
-- the difference between joining on time and joining at four past.
ALTER TABLE time_blocks ADD COLUMN link TEXT;
