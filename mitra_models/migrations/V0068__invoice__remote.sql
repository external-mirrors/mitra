ALTER TABLE invoice DROP CONSTRAINT invoice_recipient_id_fkey;
ALTER TABLE invoice ADD CONSTRAINT invoice_recipient_id_fkey FOREIGN KEY (recipient_id) REFERENCES actor_profile (id) ON DELETE CASCADE;
ALTER TABLE invoice ALTER COLUMN payment_address DROP NOT NULL;
ALTER TABLE invoice ADD COLUMN object_id VARCHAR(2000) UNIQUE;
