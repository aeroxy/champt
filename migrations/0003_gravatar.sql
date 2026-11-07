-- Set while someone chooses to show their Gravatar, null otherwise. It is
-- random, not derived from the email, so the page can fetch the picture from
-- /api/avatars/{gravatar} without learning anything about the email. Hiding
-- the picture forgets the id, and showing it again makes a new one.
ALTER TABLE people ADD COLUMN gravatar UUID UNIQUE;
