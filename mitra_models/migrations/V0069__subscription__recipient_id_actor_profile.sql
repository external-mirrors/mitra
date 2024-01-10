ALTER TABLE subscription DROP CONSTRAINT subscription_recipient_id_fkey;
ALTER TABLE subscription ADD CONSTRAINT subscription_recipient_id_fkey FOREIGN KEY (recipient_id) REFERENCES actor_profile (id) ON DELETE CASCADE;
