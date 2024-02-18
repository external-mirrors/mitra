ALTER TABLE actor_profile ADD COLUMN mention_policy SMALLINT NOT NULL DEFAULT 0;
ALTER TABLE actor_profile ALTER COLUMN mention_policy DROP DEFAULT;
