use monero_rpc::{
    HashString,
    RpcAuthentication,
    RpcClientBuilder,
    SubaddressBalanceData,
    SweepAllArgs,
    TransferPriority,
    WalletClient,
};
use monero_rpc::monero::{
    cryptonote::subaddress::Index,
    util::address::Error as AddressError,
    Address,
    Amount,
};

use mitra_config::MoneroConfig;
use mitra_models::database::DatabaseError;

use super::utils::parse_monero_address;

#[derive(thiserror::Error, Debug)]
pub enum MoneroError {
    #[error(transparent)]
    WalletError(#[from] anyhow::Error),

    #[error("{0}")]
    WalletRpcError(&'static str),

    #[error("unexpected account")]
    UnexpectedAccount,

    #[error(transparent)]
    AddressError(#[from] AddressError),

    #[error(transparent)]
    DatabaseError(#[from] DatabaseError),

    #[error("not enough unlocked balance")]
    Dust,

    #[error("{0}")]
    OtherError(&'static str),
}

fn build_wallet_client(config: &MoneroConfig)
    -> Result<WalletClient, MoneroError>
{
    let rpc_authentication = match config.wallet_rpc_username {
        Some(ref username) => {
            RpcAuthentication::Credentials {
                username: username.clone(),
                password: config.wallet_rpc_password.as_deref()
                    .unwrap_or("").to_string(),
            }
        },
        None => RpcAuthentication::None,
    };
    let wallet_client = RpcClientBuilder::new()
        .rpc_authentication(rpc_authentication)
        .build(config.wallet_rpc_url.clone())?
        .wallet();
    Ok(wallet_client)
}

/// https://monerodocs.org/interacting/monero-wallet-rpc-reference/#create_wallet
pub async fn create_monero_wallet(
    config: &MoneroConfig,
    name: String,
    password: Option<String>,
) -> Result<(), MoneroError> {
    let wallet_client = build_wallet_client(config)?;
    let language = "English".to_string();
    wallet_client.create_wallet(name, password, language).await?;
    Ok(())
}

/// https://monerodocs.org/interacting/monero-wallet-rpc-reference/#open_wallet
pub async fn open_monero_wallet(
    config: &MoneroConfig,
) -> Result<WalletClient, MoneroError> {
    let wallet_client = build_wallet_client(config)?;
    if let Err(error) = wallet_client.refresh(None).await {
        if error.to_string() == "Server error: No wallet file" {
            // Try to open wallet
            if let Some(ref wallet_name) = config.wallet_name {
                wallet_client.open_wallet(
                    wallet_name.clone(),
                    config.wallet_password.clone(),
                ).await?;
            } else {
                return Err(MoneroError::WalletRpcError("wallet file is required"));
            };
        } else {
            return Err(error.into());
        };
    };
    // Verify account exists
    let account_exists = wallet_client.get_accounts(None).await?
        .subaddress_accounts.into_iter()
        .any(|account| account.account_index == config.account_index);
    if !account_exists {
        return Err(MoneroError::WalletRpcError("account doesn't exist"));
    };
    Ok(wallet_client)
}

pub async fn create_monero_address(
    config: &MoneroConfig,
) -> Result<Address, MoneroError> {
    let wallet_client = open_monero_wallet(config).await?;
    let account_index = config.account_index;
    let (address, address_index) =
        wallet_client.create_address(account_index, None).await?;
    log::info!("created monero address {}/{}", account_index, address_index);
    // Save wallet
    wallet_client.close_wallet().await?;
    Ok(address)
}

pub async fn get_subaddress_index(
    wallet_client: &WalletClient,
    account_index: u32,
    address: &str,
) -> Result<Index, MoneroError> {
    let address = parse_monero_address(address)?;
    let address_index = wallet_client.get_address_index(address).await?;
    if address_index.major != account_index {
        return Err(MoneroError::UnexpectedAccount);
    };
    Ok(address_index)
}

fn get_single_item<T: Clone>(items: Vec<T>) -> Result<T, MoneroError> {
    if let [item] = &items[..] {
        Ok(item.clone())
    } else {
        Err(MoneroError::WalletRpcError("expected single item"))
    }
}

pub async fn get_subaddress_by_index(
    wallet_client: &WalletClient,
    subaddress_index: &Index,
) -> Result<Address, MoneroError> {
    let address_data = wallet_client.get_address(
        subaddress_index.major,
        Some(vec![subaddress_index.minor]),
    ).await?;
    let subaddress_data = get_single_item(address_data.addresses)?;
    Ok(subaddress_data.address)
}

/// https://www.getmonero.org/resources/developer-guides/wallet-rpc.html#get_balance
pub async fn get_subaddress_balance(
    wallet_client: &WalletClient,
    subaddress_index: &Index,
) -> Result<SubaddressBalanceData, MoneroError> {
    let balance_data = wallet_client.get_balance(
        subaddress_index.major,
        Some(vec![subaddress_index.minor]),
    ).await?;
    let subaddress_data = get_single_item(balance_data.per_subaddress)?;
    Ok(subaddress_data)
}

/// https://monerodocs.org/interacting/monero-wallet-rpc-reference/#sweep_all
pub async fn send_monero(
    wallet_client: &WalletClient,
    from_account: u32,
    from_address: u32,
    to_address: Address,
) -> Result<(String, Amount), MoneroError> {
    let sweep_args = SweepAllArgs {
        address: to_address,
        account_index: from_account,
        subaddr_indices: Some(vec![from_address]),
        priority: TransferPriority::Default,
        mixin: 15,
        ring_size: 16,
        unlock_time: 1,
        get_tx_keys: None,
        below_amount: None,
        do_not_relay: None,
        get_tx_hex: None,
        get_tx_metadata: None,
    };
    let sweep_data = wallet_client.sweep_all(sweep_args).await
        .map_err(|error| {
            if error.to_string() == "Server error: No unlocked balance in the specified subaddress(es)" ||
                error.to_string() == "Server error: not enough unlocked money"
            {
                MoneroError::Dust
            } else {
                error.into()
            }
        })?;
    let HashString(tx_hash) = get_single_item(sweep_data.tx_hash_list)?;
    let amount = get_single_item(sweep_data.amount_list)?;
    let fee = get_single_item(sweep_data.fee_list)?;
    log::info!(
        "sent transaction {:x} from {}/{}, amount {}, fee {}",
        tx_hash,
        from_account,
        from_address,
        amount,
        fee,
    );
    // Save wallet
    wallet_client.close_wallet().await?;
    Ok((format!("{:x}", tx_hash), amount))
}

/// https://monerodocs.org/interacting/monero-wallet-rpc-reference/#sign
pub async fn create_monero_signature(
    config: &MoneroConfig,
    message: &str,
) -> Result<(Address, String), MoneroError> {
    let wallet_client = open_monero_wallet(config).await?;
    let address_data = wallet_client.get_address(
        config.account_index,
        Some(vec![0]),
    ).await?;
    let address = address_data.address;
    let signature = wallet_client.sign(message.to_string()).await?;
    Ok((address, signature))
}

/// https://monerodocs.org/interacting/monero-wallet-rpc-reference/#verify
pub async fn verify_monero_signature(
    config: &MoneroConfig,
    address: &str,
    message: &str,
    signature: &str,
) -> Result<(), MoneroError> {
    let address = parse_monero_address(address)?;
    let wallet_client = open_monero_wallet(config).await?;
    let is_valid = wallet_client.verify(
        message.to_string(),
        address,
        signature.to_string(),
    ).await?;
    if !is_valid {
        return Err(MoneroError::OtherError("invalid signature"));
    };
    Ok(())
}
