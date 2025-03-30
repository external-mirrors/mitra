ALTER TABLE actor_profile ADD COLUMN is_automated BOOLEAN NOT NULL DEFAULT FALSE;
UPDATE actor_profile SET is_automated = coalesce(actor_json ->> 'type', '') IN ('Service', 'Application');
ALTER TABLE actor_profile ALTER COLUMN is_automated DROP DEFAULT;
