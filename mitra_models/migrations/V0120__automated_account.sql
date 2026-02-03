CREATE TABLE automated_account (
    id UUID PRIMARY KEY REFERENCES actor_profile (id) ON DELETE CASCADE,
    account_type SMALLINT UNIQUE NOT NULL,
    rsa_secret_key BYTEA NOT NULL,
    ed25519_secret_key BYTEA NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP
);

ALTER TABLE actor_profile ADD COLUMN automated_account_id UUID REFERENCES automated_account (id) ON DELETE RESTRICT;

ALTER TABLE actor_profile
    ADD CONSTRAINT actor_profile_automated_account_id_id_check
    CHECK (automated_account_id IS NULL OR automated_account_id = id);
