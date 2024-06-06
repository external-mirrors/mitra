ALTER TABLE activitypub_object ADD COLUMN profile_id UUID UNIQUE REFERENCES actor_profile (id) ON DELETE CASCADE;
