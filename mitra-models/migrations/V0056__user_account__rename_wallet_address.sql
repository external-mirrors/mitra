ALTER TABLE user_account RENAME COLUMN wallet_address TO login_address_ethereum;
ALTER TABLE user_account ALTER COLUMN login_address_ethereum TYPE VARCHAR(500);
ALTER TABLE invoice ALTER COLUMN payment_address TYPE VARCHAR(500);
ALTER TABLE subscription ALTER COLUMN sender_address TYPE VARCHAR(500);
