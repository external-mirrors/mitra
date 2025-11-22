ALTER TABLE oauth_token ADD COLUMN application_id INTEGER REFERENCES oauth_application (id) ON DELETE CASCADE;
