ALTER TABLE subscription DROP COLUMN sender_address;
ALTER TABLE subscription DROP COLUMN chain_id;

-- remove ethereum payment options
UPDATE actor_profile
SET payment_options = '[]'
WHERE
    user_id IS NOT NULL
    AND EXISTS (
        SELECT 1
        FROM jsonb_array_elements(payment_options) AS option
        WHERE CAST(option ->> 'payment_type' AS SMALLINT) = 2
    );
