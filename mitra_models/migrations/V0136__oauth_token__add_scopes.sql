ALTER TABLE oauth_authorization DROP COLUMN scopes;
ALTER TABLE oauth_authorization ADD COLUMN scopes TEXT[] NOT NULL DEFAULT '{}';
ALTER TABLE oauth_authorization ALTER COLUMN scopes DROP DEFAULT;

ALTER TABLE oauth_token ADD COLUMN scopes TEXT[] NOT NULL DEFAULT '{}';
ALTER TABLE oauth_token ALTER COLUMN scopes DROP DEFAULT;
