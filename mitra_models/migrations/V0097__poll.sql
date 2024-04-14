CREATE TABLE poll (
    id UUID PRIMARY KEY REFERENCES post (id) ON DELETE CASCADE,
    multiple_choices BOOLEAN NOT NULL,
    ends_at TIMESTAMP WITH TIME ZONE NOT NULL,
    results JSONB NOT NULL
);

CREATE TABLE poll_vote (
    id UUID PRIMARY KEY,
    poll_id UUID NOT NULL REFERENCES poll (id) ON DELETE CASCADE,
    voter_id UUID NOT NULL REFERENCES actor_profile (id) ON DELETE CASCADE,
    choice VARCHAR(1000) NOT NULL,
    UNIQUE (poll_id, voter_id, choice)
);
