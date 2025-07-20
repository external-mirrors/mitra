CREATE TABLE activitypub_media (
    owner_id UUID NOT NULL REFERENCES portable_user_account (id) ON DELETE CASCADE,
    media JSONB NOT NULL,
    digest TEXT NOT NULL GENERATED ALWAYS AS (media ->> 'digest') STORED,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (owner_id, digest)
);
