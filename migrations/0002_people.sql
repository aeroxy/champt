-- People who have signed in with Aero and chosen a nickname. `id` is the
-- author id the threads and messages already use. The email is for the
-- operator's audits and never leaves the server. Nicknames are what others
-- see, unique ignoring case.
CREATE TABLE people (
    id         UUID PRIMARY KEY,
    email      TEXT NOT NULL,
    nickname   TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX people_nickname ON people (lower(nickname));
