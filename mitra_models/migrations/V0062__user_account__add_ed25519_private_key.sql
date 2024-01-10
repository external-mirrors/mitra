ALTER TABLE user_account RENAME COLUMN private_key TO rsa_private_key;
ALTER TABLE user_account ADD COLUMN ed25519_private_key BYTEA;
