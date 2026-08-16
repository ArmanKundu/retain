-- Where the class is, and who teaches it.
--
-- Compass sends a class as three separate ICS properties: SUMMARY is the class
-- code ("11CHEU2"), LOCATION is the room ("T3"), DESCRIPTION is the teacher
-- ("Attending Staff : BGY"). Retain parsed the first two-thirds of that and
-- dropped LOCATION on the floor, so Today could only ever say "11CHEU2" —
-- a code with no room, no teacher and no subject name, which is exactly the
-- information you actually want at 8:25am.
ALTER TABLE calendar_events ADD COLUMN location TEXT;
