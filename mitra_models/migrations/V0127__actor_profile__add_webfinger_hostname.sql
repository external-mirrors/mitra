ALTER TABLE actor_profile ADD COLUMN webfinger_hostname VARCHAR(100);
UPDATE actor_profile SET webfinger_hostname = NULLIF(split_part(acct, '@', 2), '');
ALTER TABLE actor_profile DROP COLUMN acct;
ALTER TABLE actor_profile ADD COLUMN acct
    VARCHAR(200) UNIQUE
    GENERATED ALWAYS AS (
        CASE
        WHEN webfinger_hostname IS NOT NULL THEN username || '@' || webfinger_hostname
        WHEN (user_id IS NOT NULL OR portable_user_id IS NOT NULL OR automated_account_id IS NOT NULL) THEN username
        ELSE NULL
        END
    )
    STORED;
