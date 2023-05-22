use std::str::FromStr;

use monero_rpc::TransferType;
use monero_rpc::monero::Address;
use uuid::Uuid;

use mitra_config::MoneroConfig;
use mitra_models::{
    database::DatabaseClient,
    invoices::queries::{
        get_invoice_by_address,
        get_invoice_by_id,
        set_invoice_status,
    },
    invoices::types::InvoiceStatus,
};

use crate::errors::ValidationError;

use super::wallet::{
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
    db_client: &impl DatabaseClient,
    invoice_id_or_address: &str,
) -> Result<(), MoneroError> {
    let wallet_client = open_monero_wallet(config).await?;
    let invoice = if let Ok(invoice_id) = Uuid::from_str(invoice_id_or_address) {
        get_invoice_by_id(db_client, &invoice_id).await?
    } else {
        get_invoice_by_address(
            db_client,
            &config.chain_id,
            invoice_id_or_address,
        ).await?
    };
    if invoice.chain_id != config.chain_id {
        return Err(MoneroError::OtherError("can't process invoice"));
    };
    if invoice.invoice_status != InvoiceStatus::Forwarded &&
        invoice.invoice_status != InvoiceStatus::Timeout &&
        invoice.invoice_status != InvoiceStatus::Cancelled
    {
        return Err(MoneroError::OtherError("invoice is already open"));
    };
    let address = Address::from_str(&invoice.payment_address)?;
    let address_index = wallet_client.get_address_index(address).await?;
    if address_index.major != config.account_index {
        return Err(MoneroError::WalletRpcError("unexpected account index"));
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
        set_invoice_status(db_client, &invoice.id, InvoiceStatus::Paid).await?;
    };
    Ok(())
}
