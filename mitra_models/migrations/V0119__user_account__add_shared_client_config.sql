ALTER TABLE user_account ADD COLUMN shared_client_config JSONB NOT NULL DEFAULT '{}';

UPDATE user_account
SET shared_client_config = jsonb_set(
    shared_client_config,
    '{default_post_visibility}',
    to_jsonb(
        CASE client_config -> 'mitra-web' ->> 'defaultVisibility'
            WHEN 'public' THEN 1 -- public
            WHEN 'private' THEN 3 -- followers
            WHEN 'subscribers' THEN 4 -- subscribers
            ELSE 1
        END
    )
);
