ALTER TABLE subscription ALTER COLUMN chain_id DROP NOT NULL;
UPDATE subscription
    SET chain_id = NULL
    FROM actor_profile
    WHERE subscription.recipient_id = actor_profile.id AND actor_profile.actor_id IS NOT NULL;
