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

pub async fn invoice_forwarded(
    db_client: &mut impl DatabaseClient,
    invoice_id: &Uuid,
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

pub async fn invoice_reopened(
    db_client: &mut impl DatabaseClient,
    invoice_id: &Uuid,
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
    invoice_id: &Uuid,
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
    use serial_test::serial;
    use mitra_utils::caip2::ChainId;
    use crate::database::test_utils::create_test_database;
    use crate::invoices::queries::{create_invoice, create_remote_invoice};
    use crate::profiles::{
        queries::create_profile,
        types::ProfileCreateData,
    };
    use crate::users::{
        queries::create_user,
        types::UserCreateData,
    };
    use super::*;

    #[tokio::test]
    #[serial]
    async fn test_invoice_forwarded_and_reopened() {
        let db_client = &mut create_test_database().await;
        let sender_data = ProfileCreateData {
            username: "sender".to_string(),
            ..Default::default()
        };
        let sender = create_profile(db_client, sender_data).await.unwrap();
        let recipient_data = UserCreateData {
            username: "recipient".to_string(),
            password_hash: Some("test".to_string()),
            ..Default::default()
        };
        let recipient = create_user(db_client, recipient_data).await.unwrap();
        let invoice = create_invoice(
            db_client,
            &sender.id,
            &recipient.id,
            &ChainId::monero_mainnet(),
            "8MxABajuo71BZya9",
            100000000000000_u64,
        ).await.unwrap();
        set_invoice_status(
            db_client,
            &invoice.id,
            InvoiceStatus::Paid,
        ).await.unwrap();

        let payout_tx_id = "12abcd";
        let invoice = invoice_forwarded(
            db_client,
            &invoice.id,
            payout_tx_id,
        ).await.unwrap();
        assert_eq!(invoice.invoice_status, InvoiceStatus::Forwarded);
        assert_eq!(invoice.payout_tx_id.as_deref(), Some(payout_tx_id));

        set_invoice_status(
            db_client,
            &invoice.id,
            InvoiceStatus::Completed,
        ).await.unwrap();

        let invoice = invoice_reopened(
            db_client,
            &invoice.id,
        ).await.unwrap();
        assert_eq!(invoice.invoice_status, InvoiceStatus::Paid);
        assert_eq!(invoice.payout_tx_id, None);
    }

    #[tokio::test]
    #[serial]
    async fn test_remote_invoice_opened() {
        let db_client = &mut create_test_database().await;
        let sender_data = UserCreateData {
            username: "sender".to_string(),
            password_hash: Some("test".to_string()),
            ..Default::default()
        };
        let sender = create_user(db_client, sender_data).await.unwrap();
        let recipient_data = ProfileCreateData {
            username: "recipient".to_string(),
            ..Default::default()
        };
        let recipient = create_profile(db_client, recipient_data).await.unwrap();
        let invoice = create_remote_invoice(
            db_client,
            &sender.id,
            &recipient.id,
            &ChainId::monero_mainnet(),
            100000000000000_u64,
        ).await.unwrap();
        assert_eq!(invoice.invoice_status, InvoiceStatus::Requested);

        let payment_address = "8xyz";
        let object_id = "https://remote.example/objects/1";
        let invoice = remote_invoice_opened(
            db_client,
            &invoice.id,
            payment_address,
            object_id,
        ).await.unwrap();
        assert_eq!(invoice.invoice_status, InvoiceStatus::Open);
        assert_eq!(invoice.payment_address.unwrap(), payment_address);
        assert_eq!(invoice.object_id.unwrap(), object_id);
    }
}
