use std::collections::HashMap;
use std::str::FromStr;

use monero_rpc::TransferType;
use monero_rpc::monero::{Address, Amount};
use uuid::Uuid;

use mitra_config::MoneroConfig;
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    invoices::helpers::invoice_reopened,
    invoices::queries::{
        create_invoice,
        get_invoice_by_participants,
    },
    invoices::types::DbInvoice,
    profiles::types::PaymentType,
    users::queries::get_user_by_id,
};

use crate::errors::ValidationError;

use super::wallet::{
    create_monero_address,
    open_monero_wallet,
    MoneroError,
};

pub fn validate_monero_address(address: &str)
    -> Result<(), ValidationError>
{
    Address::from_str(address)
        .map_err(|_| ValidationError("invalid monero address"))?;
    Ok(())
}

pub async fn reopen_invoice(
    config: &MoneroConfig,
    db_client: &mut impl DatabaseClient,
    invoice: DbInvoice,
) -> Result<(), MoneroError> {
    if invoice.chain_id != config.chain_id {
        return Err(MoneroError::OtherError("can't process invoice"));
    };
    if !invoice.invoice_status.is_final() {
        return Err(MoneroError::OtherError("invoice is already open"));
    };
    let wallet_client = open_monero_wallet(config).await?;
    let address = Address::from_str(&invoice.payment_address)?;
    let address_index = wallet_client.get_address_index(address).await?;
    if address_index.major != config.account_index {
        // Configuration has changed
        return Err(MoneroError::OtherError("can't process invoice"));
    };

    let transfers = wallet_client.incoming_transfers(
        TransferType::Available,
        Some(address_index.major),
        Some(vec![address_index.minor]),
    ).await?
        .transfers
        .unwrap_or_default();
    if transfers.is_empty() {
        log::info!("no incoming transfers");
    } else {
        for transfer in transfers {
            if transfer.subaddr_index != address_index {
                return Err(MoneroError::WalletRpcError("unexpected transfer"));
            };
            log::info!(
                "received payment for invoice {} ({:?}): {}",
                invoice.id,
                invoice.invoice_status,
                transfer.amount,
            );
        };
        invoice_reopened(db_client, &invoice.id).await?;
    };
    Ok(())
}

pub async fn get_active_addresses(
    config: &MoneroConfig,
) -> Result<HashMap<Address, Amount>, MoneroError> {
    let wallet_client = open_monero_wallet(config).await?;
    let balance_data = wallet_client.get_balance(
        config.account_index,
        None, // all subaddresses
    ).await?;
    let mut addresses = HashMap::new();
    for subaddress_data in balance_data.per_subaddress {
        if subaddress_data.address_index == 0 {
            // Ignore account address
            continue;
        };
        if !addresses.contains_key(&subaddress_data.address) {
            addresses.insert(subaddress_data.address, subaddress_data.balance);
        };
    };
    Ok(addresses)
}

pub async fn get_payment_address(
    config: &MoneroConfig,
    db_client: &mut impl DatabaseClient,
    sender_id: &Uuid,
    recipient_id: &Uuid,
) -> Result<String, MoneroError> {
    let recipient = get_user_by_id(db_client, recipient_id).await?;
    if !recipient.profile.payment_options.any(PaymentType::MoneroSubscription) {
        return Err(MoneroError::OtherError("recipient can't accept payments"));
    };
    let invoice = match get_invoice_by_participants(
        db_client,
        sender_id,
        recipient_id,
        &config.chain_id,
    ).await {
        Ok(invoice) => invoice, // invoice will be re-opened automatically on incoming payment
        Err(DatabaseError::NotFound(_)) => {
            let payment_address = create_monero_address(config).await?;
            create_invoice(
                db_client,
                sender_id,
                recipient_id,
                &config.chain_id,
                &payment_address.to_string(),
                0, // any amount
            ).await?
        },
        Err(other_error) => return Err(other_error.into()),
    };
    Ok(invoice.payment_address)
}
