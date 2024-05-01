CREATE TABLE portable_user_account (
    id UUID PRIMARY KEY REFERENCES actor_profile (id) ON DELETE CASCADE,
    rsa_secret_key BYTEA NOT NULL,
    ed25519_secret_key BYTEA NOT NULL,
    invite_code VARCHAR(100) UNIQUE REFERENCES user_invite_code (code) ON DELETE SET NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP
);

ALTER TABLE actor_profile
    ADD COLUMN portable_user_id
    UUID UNIQUE REFERENCES portable_user_account (id) ON DELETE RESTRICT;

ALTER TABLE actor_profile
    ADD CONSTRAINT actor_profile_portable_user_id_id_check
    CHECK (portable_user_id IS NULL OR portable_user_id = id);

ALTER TABLE actor_profile DROP CONSTRAINT actor_profile_hostname_check;
