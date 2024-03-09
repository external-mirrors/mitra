ALTER TABLE notification ADD COLUMN reaction_id UUID REFERENCES post_reaction (id) ON DELETE CASCADE;
