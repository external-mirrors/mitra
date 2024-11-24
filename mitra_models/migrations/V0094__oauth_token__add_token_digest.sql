ALTER TABLE user_account RENAME COLUMN password_hash TO password_digest;

ALTER TABLE oauth_token ALTER COLUMN token DROP NOT NULL;
ALTER TABLE oauth_token ADD COLUMN token_digest BYTEA UNIQUE;
ALTER TABLE oauth_token
    ADD CONSTRAINT oauth_token_token_token_digest_check
    CHECK (token IS NOT NULL OR token_digest IS NOT NULL);
