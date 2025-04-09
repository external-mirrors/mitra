DELETE FROM oauth_token WHERE token_digest IS NULL;
ALTER TABLE oauth_token ALTER COLUMN token_digest SET NOT NULL;
ALTER TABLE oauth_token DROP COLUMN token;
