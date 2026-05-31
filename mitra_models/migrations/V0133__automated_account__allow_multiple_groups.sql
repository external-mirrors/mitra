ALTER TABLE automated_account DROP CONSTRAINT automated_account_account_type_key;
CREATE UNIQUE INDEX automated_account_account_type_idx ON automated_account (account_type) WHERE account_type != 4;
