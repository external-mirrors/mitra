ALTER TABLE oauth_application DROP COLUMN scopes;
ALTER TABLE oauth_application ADD COLUMN scopes TEXT[] NOT NULL DEFAULT '{"read", "write"}';
ALTER TABLE oauth_application ALTER COLUMN scopes DROP DEFAULT;
