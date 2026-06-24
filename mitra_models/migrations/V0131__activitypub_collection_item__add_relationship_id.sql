ALTER TABLE activitypub_collection_item ADD COLUMN relationship_id INTEGER REFERENCES relationship (id) ON DELETE CASCADE;
