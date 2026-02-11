use anyhow::{Error as LwsError};
use chrono::{DateTime, Utc};
use monero::util::{
    address::{Address, PaymentId},
    key::PrivateKey,
};
use monero_lws::{
    LwsRpcClient,
};
use thiserror::Error;

use mitra_config::MoneroLightConfig;

pub struct LightWalletClient {
    client: LwsRpcClient,
    address: Address,
    view_key: PrivateKey,
}

#[derive(Debug, Error)]
pub enum LightWalletError {
    #[error(transparent)]
    ApiError(#[from] LwsError),

    #[error("unexpected response")]
    UnexpectedResponse,
}

impl LightWalletClient {
    pub fn new(
        config: &MoneroLightConfig,
        address: Address,
        view_key: PrivateKey,
    ) -> Self {
        let client = LwsRpcClient::new(config.lightwallet_api_url.clone());
        Self { client, address, view_key }
    }

    // https://github.com/monero-project/meta/blob/master/api/lightwallet_rest.md#get_address_txs
    pub async fn get_tx_id_by_payment_id(
        &self,
        payment_id: PaymentId,
    ) -> Result<Option<String>, LightWalletError> {
        let response = self.client.get_address_txs(
            self.address,
            self.view_key,
        ).await?;
        let maybe_tx_id = response.transactions
            .iter()
            // Ignore spends
            .filter(|tx| tx.total_received != "0")
            .find(|tx| tx.payment_id.as_ref().is_some_and(|id| id.0 == payment_id))
            .map(|tx| tx.hash.to_string());
        Ok(maybe_tx_id)
    }

    pub async fn get_tx_info(
        &self,
        tx_id: &str,
    ) -> Result<(u64, u64), LightWalletError> {
        let response = self.client.get_address_txs(
            self.address,
            self.view_key,
        ).await?;
        // `blockchain_height` contains a wrong value
        let blockchain_height = response.scanned_block_height;
        let maybe_tx_info = response.transactions
            .iter()
            .find(|tx| tx.hash.to_string() == tx_id);
        let Some(tx_info) = maybe_tx_info else {
            return Err(LightWalletError::UnexpectedResponse);
        };
        let tx_height = tx_info.height.unwrap_or(blockchain_height);
        let confirmations = blockchain_height.checked_sub(tx_height)
            .ok_or(LightWalletError::UnexpectedResponse)?;
        let amount = tx_info.total_received
            .parse::<u64>()
            .map_err(|_| LightWalletError::UnexpectedResponse)?;
        Ok((amount, confirmations))
    }

    pub async fn get_primary_address_txs(
        &self,
        since_date: DateTime<Utc>,
    ) -> Result<Vec<String>, LightWalletError> {
        let response = self.client.get_address_txs(
            self.address,
            self.view_key,
        ).await?;
        let mut primary_address_tx_ids = vec![];
        for transaction in response.transactions {
            if transaction.total_received == "0" {
                // Ignore spends
                continue;
            };
            if transaction.payment_id.is_some_and(|id| id.0 != PaymentId::zero()) {
                // Not a primary address
                continue;
            };
            let timestamp = DateTime::parse_from_rfc3339(&transaction.timestamp)
                .map_err(|_| LightWalletError::UnexpectedResponse)?;
            if timestamp <= since_date {
                continue;
            };
            primary_address_tx_ids.push(transaction.hash.to_string());
        };
        Ok(primary_address_tx_ids)
    }

    // https://github.com/monero-project/meta/blob/master/api/lightwallet_rest.md#login
    pub async fn login(
        &self,
    ) -> Result<(), LightWalletError> {
        let _ = self.client.login(
            self.address,
            self.view_key,
            true, // create account
            true,  // generated locally
        ).await?;
        Ok(())
    }
}
