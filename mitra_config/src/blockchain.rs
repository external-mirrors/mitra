use apx_core::caip2::ChainId;
use serde::Deserialize;

fn default_wallet_account_index() -> u32 { 0 }

#[derive(Clone, Default, Deserialize)]
pub struct MoneroChainMetadata {
    pub description: Option<String>,
}

#[derive(Clone, Deserialize)]
pub struct MoneroConfig {
    pub chain_id: ChainId,
    // Additional information for clients
    pub chain_metadata: Option<MoneroChainMetadata>,
    pub wallet_rpc_url: String,
    pub wallet_rpc_username: Option<String>,
    pub wallet_rpc_password: Option<String>,
    // Wallet name and password are required when
    // monero-wallet-rpc is running with --wallet-dir option
    pub wallet_name: Option<String>,
    pub wallet_password: Option<String>,
    #[serde(default = "default_wallet_account_index")]
    pub account_index: u32,
}

#[derive(Clone, Deserialize)]
#[serde(untagged)]
pub enum BlockchainConfig {
    Monero(MoneroConfig),
}
