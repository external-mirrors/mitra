ALTER TABLE actor_profile DROP COLUMN acct;
ALTER TABLE actor_profile ADD COLUMN acct VARCHAR(200) UNIQUE;
UPDATE actor_profile SET acct = (CASE WHEN hostname IS NULL THEN username ELSE username || '@' || hostname END);
