CREATE TABLE payment_method (
    id INTEGER GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    owner_id UUID NOT NULL REFERENCES user_account (id) ON DELETE CASCADE,
    payment_type SMALLINT NOT NULL,
    chain_id VARCHAR(50) NOT NULL,
    payout_address VARCHAR(500) NOT NULL,
    UNIQUE (owner_id, chain_id)
);

INSERT INTO payment_method (
    owner_id,
    payment_type,
    chain_id,
    payout_address
)
SELECT
    actor_profile.id,
    1, -- Monero
    payment_option ->> 'chain_id',
    payment_option ->> 'payout_address'
FROM actor_profile
CROSS JOIN jsonb_array_elements(actor_profile.payment_options) AS payment_option
-- MoneroSubscription
WHERE (payment_option -> 'payment_type')::integer = 3;

ALTER TABLE invoice ADD COLUMN payment_type SMALLINT;

-- delete invoices that are linked to deprecated payment methods
DELETE FROM invoice
WHERE chain_id NOT LIKE 'monero%';

-- set payment type to 'Monero' for all local invoices
UPDATE invoice
SET payment_type = 1 -- payment_type: Monero
WHERE
    object_id IS NULL
    -- status: Requested
    AND invoice_status != 9;
