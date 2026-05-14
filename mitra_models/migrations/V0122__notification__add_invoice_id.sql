ALTER TABLE invoice ADD COLUMN payout_amount BIGINT CHECK (payout_amount > 0);

ALTER TABLE notification ADD COLUMN invoice_id UUID REFERENCES invoice (id) ON DELETE CASCADE;
