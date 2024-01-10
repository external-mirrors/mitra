UPDATE invoice
SET chain_id = CASE
    WHEN chain_id = 'monero:mainnet'
    THEN 'monero:418015bb9ae982a1975da7d79277c270'
    WHEN chain_id = 'monero:stagenet'
    THEN 'monero:76ee3cc98646292206cd3e86f74d88b4'
    WHEN chain_id = 'monero:testnet'
    THEN 'monero:48ca7cd3c8de5b6a4d53d2861fbdaedc'
    WHEN chain_id = 'monero:regtest'
    THEN 'monero:00000000000000000000000000000000'
    END
WHERE starts_with(chain_id, 'monero:');

UPDATE subscription
SET chain_id = CASE
    WHEN chain_id = 'monero:mainnet'
    THEN 'monero:418015bb9ae982a1975da7d79277c270'
    WHEN chain_id = 'monero:stagenet'
    THEN 'monero:76ee3cc98646292206cd3e86f74d88b4'
    WHEN chain_id = 'monero:testnet'
    THEN 'monero:48ca7cd3c8de5b6a4d53d2861fbdaedc'
    WHEN chain_id = 'monero:regtest'
    THEN 'monero:00000000000000000000000000000000'
    END
WHERE starts_with(chain_id, 'monero:');

UPDATE caip122_nonce
SET account_id = CASE
    WHEN starts_with(account_id, 'monero:mainnet')
    THEN replace(account_id, 'monero:mainnet', 'monero:418015bb9ae982a1975da7d79277c270')
    WHEN starts_with(account_id, 'monero:stagenet')
    THEN replace(account_id, 'monero:stagenet', 'monero:76ee3cc98646292206cd3e86f74d88b4')
    WHEN starts_with(account_id, 'monero:testnet')
    THEN replace(account_id, 'monero:testnet', 'monero:48ca7cd3c8de5b6a4d53d2861fbdaedc')
    WHEN starts_with(account_id, 'monero:regtest')
    THEN replace(account_id, 'monero:regtest', 'monero:00000000000000000000000000000000')
    END
WHERE starts_with(account_id, 'monero:');

UPDATE actor_profile
SET payment_options = replaced.payment_options
FROM (
    SELECT
        actor_profile.id,
        jsonb_agg(
            CASE
            WHEN payment_option ->> 'chain_id' = 'monero:mainnet'
            THEN jsonb_set(payment_option, '{chain_id}', '"monero:418015bb9ae982a1975da7d79277c270"')
            WHEN payment_option ->> 'chain_id' = 'monero:stagenet'
            THEN jsonb_set(payment_option, '{chain_id}', '"monero:76ee3cc98646292206cd3e86f74d88b4"')
            WHEN payment_option ->> 'chain_id' = 'monero:testnet'
            THEN jsonb_set(payment_option, '{chain_id}', '"monero:48ca7cd3c8de5b6a4d53d2861fbdaedc"')
            WHEN payment_option ->> 'chain_id' = 'monero:regtest'
            THEN jsonb_set(payment_option, '{chain_id}', '"monero:00000000000000000000000000000000"')
            ELSE payment_option
            END
        ) AS payment_options
    FROM actor_profile
    CROSS JOIN jsonb_array_elements(actor_profile.payment_options) AS payment_option
    GROUP BY actor_profile.id
) AS replaced
WHERE actor_profile.id = replaced.id;
