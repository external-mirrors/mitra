ALTER TABLE activitypub_collection_item DROP CONSTRAINT activitypub_collection_item_owner_id_fkey;
ALTER TABLE activitypub_collection_item ADD CONSTRAINT activitypub_collection_item_owner_id_fkey FOREIGN KEY (owner_id) REFERENCES actor_profile (id) ON DELETE CASCADE;
