use uuid::Uuid;

use crate::database::{
    DatabaseClient,
    DatabaseError,
};

use super::queries::{
    set_invoice_payout_tx_id,
    set_invoice_status,
    set_remote_invoice_data,
};
use super::types::{DbInvoice, InvoiceStatus};

pub async fn local_invoice_forwarded(
    db_client: &mut impl DatabaseClient,
    invoice_id: Uuid,
    payout_tx_id: &str,
) -> Result<DbInvoice, DatabaseError> {
    let mut transaction = db_client.transaction().await?;
    set_invoice_payout_tx_id(
        &transaction,
        invoice_id,
        Some(payout_tx_id),
    ).await?;
    let invoice = set_invoice_status(
        &mut transaction,
        invoice_id,
        InvoiceStatus::Forwarded,
    ).await?;
    transaction.commit().await?;
    Ok(invoice)
}

pub async fn local_invoice_reopened(
    db_client: &mut impl DatabaseClient,
    invoice_id: Uuid,
) -> Result<DbInvoice, DatabaseError> {
    let mut transaction = db_client.transaction().await?;
    set_invoice_payout_tx_id(
        &transaction,
        invoice_id,
        None, // reset
    ).await?;
    let invoice = set_invoice_status(
        &mut transaction,
        invoice_id,
        InvoiceStatus::Paid,
    ).await?;
    transaction.commit().await?;
    Ok(invoice)
}

pub async fn remote_invoice_opened(
    db_client: &mut impl DatabaseClient,
    invoice_id: Uuid,
    payment_address: &str,
    object_id: &str,
) -> Result<DbInvoice, DatabaseError> {
    let mut transaction = db_client.transaction().await?;
    set_remote_invoice_data(
        &transaction,
        invoice_id,
        payment_address,
        object_id,
    ).await?;
    let invoice = set_invoice_status(
        &mut transaction,
        invoice_id,
        InvoiceStatus::Open,
    ).await?;
    transaction.commit().await?;
    Ok(invoice)
}

#[cfg(test)]
mod tests {
    use apx_core::caip2::ChainId;
    use serial_test::serial;
    use crate::{
        database::test_utils::create_test_database,
        invoices::queries::{create_local_invoice, create_remote_invoice},
        profiles::test_utils::create_test_remote_profile,
        users::test_utils::create_test_user,
    };
    use super::*;

    #[tokio::test]
    #[serial]
    async fn test_local_invoice_forwarded_and_reopened() {
        let db_client = &mut create_test_database().await;
        let sender = create_test_remote_profile(
            db_client,
            "sender",
            "social.example",
            "https://social.example/actors/1",
        ).await;
        let recipient = create_test_user(db_client, "recipient").await;
        let invoice = create_local_invoice(
            db_client,
            sender.id,
            recipient.id,
            &ChainId::monero_mainnet(),
            "8MxABajuo71BZya9",
            100000000000000_u64,
        ).await.unwrap();
        set_invoice_status(
            db_client,
            invoice.id,
            InvoiceStatus::Paid,
        ).await.unwrap();

        let payout_tx_id = "12abcd";
        let invoice = local_invoice_forwarded(
            db_client,
            invoice.id,
            payout_tx_id,
        ).await.unwrap();
        assert_eq!(invoice.invoice_status, InvoiceStatus::Forwarded);
        assert_eq!(invoice.payout_tx_id.as_deref(), Some(payout_tx_id));

        set_invoice_status(
            db_client,
            invoice.id,
            InvoiceStatus::Completed,
        ).await.unwrap();

        let invoice = local_invoice_reopened(
            db_client,
            invoice.id,
        ).await.unwrap();
        assert_eq!(invoice.invoice_status, InvoiceStatus::Paid);
        assert_eq!(invoice.payout_tx_id, None);
    }

    #[tokio::test]
    #[serial]
    async fn test_remote_invoice_opened() {
        let db_client = &mut create_test_database().await;
        let sender = create_test_user(db_client, "sender").await;
        let recipient = create_test_remote_profile(
            db_client,
            "recipient",
            "social.example",
            "https://social.example/actors/1",
        ).await;
        let invoice = create_remote_invoice(
            db_client,
            sender.id,
            recipient.id,
            &ChainId::monero_mainnet(),
            100000000000000_u64,
        ).await.unwrap();
        assert_eq!(invoice.invoice_status, InvoiceStatus::Requested);

        let payment_address = "8xyz";
        let object_id = "https://remote.example/objects/1";
        let invoice = remote_invoice_opened(
            db_client,
            invoice.id,
            payment_address,
            object_id,
        ).await.unwrap();
        assert_eq!(invoice.invoice_status, InvoiceStatus::Open);
        assert_eq!(invoice.payment_address.unwrap(), payment_address);
        assert_eq!(invoice.object_id.unwrap(), object_id);
    }
}
