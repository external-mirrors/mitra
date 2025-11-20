use thiserror::Error;
use uuid::Uuid;

use mitra_config::MoneroConfig;
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    invoices::helpers::local_invoice_reopened,
    invoices::queries::{
        create_local_invoice,
        get_invoice_by_participants,
    },
    invoices::types::Invoice,
    users::queries::get_user_by_id,
};
use mitra_services::monero::{
    wallet::{
        create_monero_address,
        get_incoming_transfers,
        get_subaddress_index,
        open_monero_wallet,
        MoneroError,
    },
    utils::BLOCK_TIME,
};

const MONERO_INVOICE_WAIT_TIME: u32 = 3 * 60 * 60; // 3 hours
pub const MONERO_INVOICE_TIMEOUT: u32 = MONERO_INVOICE_WAIT_TIME + 2 * 20 * (BLOCK_TIME as u32);

#[derive(Debug, Error)]
pub enum PaymentError {
    #[error(transparent)]
    MoneroError(#[from] MoneroError),

    #[error(transparent)]
    DatabaseError(#[from] DatabaseError),
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
        return Err(MoneroError::OtherError("can't process invoice").into());
    };
    if !invoice.invoice_status.is_final() {
        return Err(MoneroError::OtherError("invoice is already open").into());
    };
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
        return Err(MoneroError::OtherError("recipient can't accept payments").into());
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
            create_local_invoice(
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
}
