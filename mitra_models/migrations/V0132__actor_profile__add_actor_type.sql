ALTER TABLE actor_profile ADD COLUMN actor_type SMALLINT NOT NULL DEFAULT 1;
UPDATE actor_profile SET actor_type = 2 WHERE is_automated IS TRUE;
UPDATE actor_profile SET actor_type = 3 WHERE actor_json ->> 'type' = 'Group';
ALTER TABLE actor_profile DROP COLUMN is_automated;
ALTER TABLE actor_profile ALTER COLUMN actor_type DROP DEFAULT;
