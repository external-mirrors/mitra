ALTER TABLE actor_profile
    ADD COLUMN user_id UUID UNIQUE REFERENCES user_account (id) ON DELETE RESTRICT;
ALTER TABLE actor_profile
    ADD CONSTRAINT actor_profile_user_id_id_check
    CHECK (user_id IS NULL OR user_id = id);

UPDATE actor_profile
SET user_id = user_account.id
FROM user_account
WHERE user_account.id = actor_profile.id;
