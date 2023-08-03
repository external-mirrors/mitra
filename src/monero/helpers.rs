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

use super::wallet::{
    create_monero_address,
    get_incoming_transfers,
    get_subaddress_index,
    open_monero_wallet,
    MoneroError,
};

pub async fn reopen_invoice(
    config: &MoneroConfig,
    db_client: &mut impl DatabaseClient,
    invoice: &DbInvoice,
) -> Result<(), MoneroError> {
    if invoice.chain_id != config.chain_id {
        return Err(MoneroError::OtherError("can't process invoice"));
    };
    if !invoice.invoice_status.is_final() {
        return Err(MoneroError::OtherError("invoice is already open"));
    };
    let wallet_client = open_monero_wallet(config).await?;
    let address_index = get_subaddress_index(
        &wallet_client,
        config.account_index,
        &invoice.payment_address,
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
        invoice_reopened(db_client, &invoice.id).await?;
    };
    Ok(())
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
