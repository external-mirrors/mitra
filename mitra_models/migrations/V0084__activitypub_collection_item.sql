CREATE TABLE activitypub_collection_item (
    owner_id UUID NOT NULL REFERENCES portable_user_account (id) ON DELETE CASCADE,
    collection_id VARCHAR(2000) NOT NULL,
    object_id VARCHAR(2000) NOT NULL REFERENCES activitypub_object (object_id) ON DELETE CASCADE,
    UNIQUE (collection_id, object_id)
);
