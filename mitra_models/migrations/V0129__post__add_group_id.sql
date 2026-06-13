ALTER TABLE post ADD COLUMN group_id UUID REFERENCES actor_profile (id) ON DELETE CASCADE;
ALTER TABLE conversation ADD COLUMN group_id UUID REFERENCES actor_profile (id) ON DELETE CASCADE;
CREATE INDEX post_group_id_index ON post (group_id);
