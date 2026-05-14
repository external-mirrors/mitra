use apx_core::caip2::{ChainId, MoneroNetwork};
use thiserror::Error;
use uuid::Uuid;

use mitra_config::{
    BlockchainConfig,
    Config,
    MoneroConfig,
};
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    invoices::helpers::local_invoice_reopened,
    invoices::queries::{
        create_local_invoice,
        get_local_invoice_by_participants,
    },
    invoices::types::Invoice,
    payment_methods::{
        helpers::get_payment_method_by_type_and_chain_id,
        types::{PaymentMethod, PaymentType},
    },
    users::queries::get_user_by_id,
};
use mitra_services::monero::{
    light_wallet::LightWalletError,
    wallet::{
        create_monero_address,
        get_incoming_transfers,
        get_subaddress_index,
        open_monero_wallet,
        MoneroError,
    },
    utils::{
        address_network,
        create_integrated_address,
        parse_monero_address,
        parse_monero_view_key,
        Address,
        AddressType,
        PrivateKey,
        BLOCK_TIME,
    },
};
use mitra_validators::errors::ValidationError;

const MONERO_INVOICE_WAIT_TIME: u32 = 3 * 60 * 60; // 3 hours
pub const MONERO_INVOICE_TIMEOUT: u32 = MONERO_INVOICE_WAIT_TIME + 2 * 20 * (BLOCK_TIME as u32);

#[derive(Debug, Error)]
pub enum PaymentError {
    #[error(transparent)]
    MoneroError(#[from] MoneroError),

    #[error(transparent)]
    LightWalletError(#[from] LightWalletError),

    #[error(transparent)]
    DatabaseError(#[from] DatabaseError),
}

fn is_valid_network(
    network: MoneroNetwork,
    expected_chain_id: &ChainId,
) -> bool {
    match expected_chain_id.monero_network() {
        Err(_) => false, // not Monero
        Ok(MoneroNetwork::Private) => true, // always valid
        Ok(expected_network) => network == expected_network,
    }
}

pub fn validate_monero_address(
    address: &str,
    expected_chain_id: &ChainId,
) -> Result<(), ValidationError> {
    let address = parse_monero_address(address)
        .map_err(|_| ValidationError("invalid monero address"))?;
    let network = address_network(address);
    if !is_valid_network(network, expected_chain_id) {
        return Err(ValidationError("address belongs to wrong network"));
    };
    Ok(())
}

pub fn validate_monero_standard_address(
    address: &str,
    expected_chain_id: &ChainId,
) -> Result<(), ValidationError> {
    let address = parse_monero_address(address)
        .map_err(|_| ValidationError("invalid monero address"))?;
    if address.addr_type != AddressType::Standard {
        return Err(ValidationError("address is not a standard address"));
    };
    let network = address_network(address);
    if !is_valid_network(network, expected_chain_id) {
        return Err(ValidationError("address belongs to wrong network"));
    };
    Ok(())
}

pub fn payment_method_payout_address(
    payment_method: &PaymentMethod,
) -> Result<Address, DatabaseError> {
    let address_str = &payment_method.payout_address;
    let address = parse_monero_address(address_str)
        .map_err(|_| DatabaseError::type_error())?;
    Ok(address)
}

pub fn payment_method_view_key(
    payment_method: &PaymentMethod,
) -> Result<PrivateKey, DatabaseError> {
    let view_key_str = payment_method.view_key
        .as_ref()
        .ok_or(DatabaseError::type_error())?;
    let view_key = parse_monero_view_key(view_key_str)
        .map_err(|_| DatabaseError::type_error())?;
    Ok(view_key)
}

pub async fn create_payment_address(
    config: &Config,
    payment_method: &PaymentMethod,
) -> Result<String, PaymentError> {
    let blockchain_config = config.blockchains().iter()
        .find(|bc_config| {
            let (payment_type, chain_id) = match bc_config {
                BlockchainConfig::Monero(monero_config) =>
                    (PaymentType::Monero, &monero_config.chain_id),
                BlockchainConfig::MoneroLight(monero_config) =>
                    (PaymentType::MoneroLight, &monero_config.chain_id),
            };
            payment_type == payment_method.payment_type
                && chain_id == payment_method.chain_id.inner()
        })
        .ok_or(MoneroError::OtherError("recipient can't accept payment"))?;
    let payment_address = match blockchain_config {
        BlockchainConfig::Monero(monero_config) => {
            create_monero_address(monero_config).await?
                .to_string()
        },
        BlockchainConfig::MoneroLight(_) => {
            let payout_address = payment_method_payout_address(payment_method)?;
            // Generate view-only integrated address
            let payment_address = create_integrated_address(payout_address);
            payment_address.to_string()
        },
    };
    Ok(payment_address)
}

pub fn invoice_payment_address(invoice: &Invoice)
    -> Result<String, DatabaseError>
{
    invoice.try_payment_address().map_err(Into::into)
}

pub async fn reopen_local_invoice(
    config: &MoneroConfig,
    db_client: &mut impl DatabaseClient,
    invoice: &Invoice,
) -> Result<(), PaymentError> {
    if invoice.chain_id != config.chain_id {
        return Err(MoneroError::OtherError("unexpected chain ID").into());
    };
    if !invoice.invoice_status.is_final() {
        return Err(MoneroError::OtherError("invoice is already open").into());
    };
    let _payment_method = get_payment_method_by_type_and_chain_id(
        db_client,
        invoice.recipient_id,
        PaymentType::Monero,
        invoice.chain_id.inner(),
    )
        .await?
        .ok_or(MoneroError::OtherError("recipient can't accept payment"))?;
    let wallet_client = open_monero_wallet(config).await?;
    let payment_address = invoice_payment_address(invoice)?;
    let address_index = get_subaddress_index(
        &wallet_client,
        config.account_index,
        &payment_address,
    ).await?;

    let transfers = get_incoming_transfers(
        &wallet_client,
        address_index.major,
        vec![address_index.minor],
    ).await?;
    if transfers.is_empty() {
        log::info!("no incoming transfers");
    } else {
        for transfer in transfers {
            log::info!(
                "received payment for invoice {} ({:?}): {}",
                invoice.id,
                invoice.invoice_status,
                transfer.amount,
            );
        };
        local_invoice_reopened(db_client, invoice.id).await?;
    };
    Ok(())
}

pub async fn get_payment_address(
    config: &MoneroConfig,
    db_client: &mut impl DatabaseClient,
    sender_id: Uuid,
    recipient_id: Uuid,
) -> Result<String, PaymentError> {
    let recipient = get_user_by_id(db_client, recipient_id).await?;
    if recipient.profile.monero_subscription(&config.chain_id).is_none() {
        return Err(MoneroError::OtherError("recipient can't accept payment").into());
    };
    let payment_method = get_payment_method_by_type_and_chain_id(
        db_client,
        recipient_id,
        PaymentType::Monero,
        &config.chain_id,
    )
        .await?
        .ok_or(MoneroError::OtherError("recipient can't accept payment"))?;
    let invoice = match get_local_invoice_by_participants(
        db_client,
        sender_id,
        recipient_id,
        payment_method.payment_type,
        payment_method.chain_id.inner(),
    ).await {
        Ok(invoice) => invoice, // invoice will be re-opened automatically on incoming payment
        Err(DatabaseError::NotFound(_)) => {
            let payment_address = create_monero_address(config).await?;
            create_local_invoice(
                db_client,
                sender_id,
                recipient_id,
                payment_method.payment_type,
                payment_method.chain_id.inner(),
                &payment_address.to_string(),
                0, // any amount
            ).await?
        },
        Err(other_error) => return Err(other_error.into()),
    };
    let payment_address = invoice_payment_address(&invoice)?;
    Ok(payment_address)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_monero_timeout() {
        assert_eq!(MONERO_INVOICE_TIMEOUT, 15600);
    }

    #[test]
    fn test_validate_monero_address_invalid() {
        let address = "1";
        let chain_id = ChainId::monero_mainnet();
        let error = validate_monero_address(address, &chain_id).err().unwrap();
        assert_eq!(error.to_string(), "invalid monero address");
    }

    #[test]
    fn test_validate_monero_address_wrong_network() {
        let address = "9uyhAgKT5tE2tyXQ9Noqga5uksHW4RrcnaU7suiiJqvL6y3domT3k8eJEiqehCsrXjCJi6Haa73AXY9eiEgCSatZM8tmwEm";
        let chain_id = ChainId::monero_mainnet();
        let error = validate_monero_address(address, &chain_id).err().unwrap();
        assert_eq!(error.to_string(), "address belongs to wrong network");
    }
}
