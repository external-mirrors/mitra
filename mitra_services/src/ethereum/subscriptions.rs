use std::convert::TryInto;

use chrono::{DateTime, TimeZone, Utc};
use web3::{
    api::Web3,
    contract::{Contract, Options},
    ethabi::RawLog,
    transports::Http,
    types::{BlockId, BlockNumber, FilterBuilder, U256},
};

use mitra_config::EthereumConfig;

use super::contracts::ContractSet;
use super::errors::EthereumError;
use super::signatures::{
    encode_uint256,
    sign_contract_call,
    CallArgs,
    SignatureData,
};
use super::utils::{address_to_string, parse_address};

fn u256_to_date(value: U256) -> Result<DateTime<Utc>, EthereumError> {
    let timestamp: i64 = value.try_into()
        .map_err(|_| EthereumError::ConversionError)?;
    let datetime = Utc.timestamp_opt(timestamp, 0)
        .single()
        .ok_or(EthereumError::ConversionError)?;
    Ok(datetime)
}

pub struct SubscriptionEvent {
    pub sender_address: String,
    pub recipient_address: String,
    pub block_date: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

/// Search for subscription update events
pub async fn get_subscription_events(
    web3: &Web3<Http>,
    contract: &Contract<Http>,
    from_block: u64,
    to_block: u64,
) -> Result<Vec<SubscriptionEvent>, EthereumError> {
    let event_abi = contract.abi().event("UpdateSubscription")?;
    let filter = FilterBuilder::default()
        .address(vec![contract.address()])
        .topics(Some(vec![event_abi.signature()]), None, None, None)
        .from_block(BlockNumber::Number(from_block.into()))
        .to_block(BlockNumber::Number(to_block.into()))
        .build();
    let logs = web3.eth().logs(filter).await?;
    let mut events = vec![];
    for log in logs {
        let block_number = if let Some(block_number) = log.block_number {
            block_number
        } else {
            // Skips logs without block number
            continue;
        };
        let raw_log = RawLog {
            topics: log.topics.clone(),
            data: log.data.clone().0,
        };
        let event = event_abi.parse_log(raw_log)?;
        let sender_address = event.params[0].value.clone().into_address()
            .map(address_to_string)
            .ok_or(EthereumError::ConversionError)?;
        let recipient_address = event.params[1].value.clone().into_address()
            .map(address_to_string)
            .ok_or(EthereumError::ConversionError)?;
        let expires_at_timestamp = event.params[2].value.clone().into_uint()
            .ok_or(EthereumError::ConversionError)?;
        let expires_at = u256_to_date(expires_at_timestamp)
            .map_err(|_| EthereumError::ConversionError)?;
        let block_id = BlockId::Number(BlockNumber::Number(block_number));
        let block_timestamp = web3.eth().block(block_id).await?
            .ok_or(EthereumError::ConversionError)?
            .timestamp;
        let block_date = u256_to_date(block_timestamp)
            .map_err(|_| EthereumError::ConversionError)?;
        events.push(SubscriptionEvent {
            sender_address,
            recipient_address,
            block_date,
            expires_at,
        });
    };
    Ok(events)
}

pub fn create_subscription_signature(
    blockchain_config: &EthereumConfig,
    user_address: &str,
    price: u64,
) -> Result<SignatureData, EthereumError> {
    let user_address = parse_address(user_address)?;
    let call_args: CallArgs = vec![
        Box::new(user_address),
        Box::new(encode_uint256(price)),
    ];
    let signature = sign_contract_call(
        &blockchain_config.signing_key,
        blockchain_config.ethereum_chain_id(),
        &blockchain_config.contract_address,
        "configureSubscription",
        call_args,
    )?;
    Ok(signature)
}

pub async fn is_registered_recipient(
    contract_set: &ContractSet,
    user_address: &str,
) -> Result<bool, EthereumError> {
    let adapter = match &contract_set.subscription_adapter {
        Some(contract) => contract,
        None => return Ok(false),
    };
    let user_address = parse_address(user_address)?;
    let result: bool = adapter.query(
        "isSubscriptionConfigured", (user_address,),
        None, Options::default(), None,
    ).await?;
    Ok(result)
}
