-- A thread is a conversation pinned to a place. It is alive while
-- expires_at is in the future; every read filters on that, so an expired
-- thread disappears at once even before it is deleted.
CREATE TABLE threads (
    id            BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    lat           DOUBLE PRECISION NOT NULL,
    lng           DOUBLE PRECISION NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at    TIMESTAMPTZ NOT NULL,
    -- Who posted last. A post resets expires_at only when its author
    -- differs from this one.
    last_author   UUID NOT NULL,
    -- The opening message, shortened, for the map's thread list.
    preview       TEXT NOT NULL,
    message_count INTEGER NOT NULL
);

CREATE INDEX threads_expires_at ON threads (expires_at);
CREATE INDEX threads_lat_lng ON threads (lat, lng);

CREATE TABLE messages (
    id          BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    thread_id   BIGINT NOT NULL REFERENCES threads (id) ON DELETE CASCADE,
    author_id   UUID NOT NULL,
    author_name TEXT NOT NULL,
    body        TEXT NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX messages_thread ON messages (thread_id, id);
