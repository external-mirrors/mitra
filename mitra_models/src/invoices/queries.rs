use std::collections::HashMap;

use apx_core::caip2::ChainId;
use uuid::Uuid;

use mitra_utils::id::generate_ulid;

use crate::database::{
    catch_unique_violation,
    DatabaseClient,
    DatabaseError,
    DatabaseTypeError,
};

use super::types::{DbChainId, Invoice, InvoiceStatus};

/// Create invoice with local recipient
pub async fn create_local_invoice(
    db_client: &impl DatabaseClient,
    sender_id: Uuid,
    recipient_id: Uuid,
    chain_id: &ChainId,
    payment_address: &str,
    amount: u64,
) -> Result<Invoice, DatabaseError> {
    let invoice_id = generate_ulid();
    let db_amount = i64::try_from(amount)
        .map_err(|_| DatabaseTypeError)?;
    let row = db_client.query_one(
        "
        INSERT INTO invoice (
            id,
            sender_id,
            recipient_id,
            chain_id,
            payment_address,
            amount
        )
        SELECT $1, $2, $3, $4, $5, $6
        WHERE EXISTS (
            -- local recipient
            SELECT 1 FROM user_account WHERE id = $3
        )
        RETURNING invoice
        ",
        &[
            &invoice_id,
            &sender_id,
            &recipient_id,
            &DbChainId::new(chain_id),
            &payment_address,
            &db_amount,
        ],
    ).await.map_err(catch_unique_violation("invoice"))?;
    let invoice = row.try_get("invoice")?;
    Ok(invoice)
}

pub async fn create_remote_invoice(
    db_client: &impl DatabaseClient,
    sender_id: Uuid,
    recipient_id: Uuid,
    chain_id: &ChainId,
    amount: u64,
) -> Result<Invoice, DatabaseError> {
    let invoice_id = generate_ulid();
    let db_amount: i64 = i64::try_from(amount)
        .map_err(|_| DatabaseTypeError)?;
    let row = db_client.query_one(
        "
        INSERT INTO invoice (
            id,
            sender_id,
            recipient_id,
            chain_id,
            amount,
            invoice_status
        )
        SELECT $1, $2, $3, $4, $5, $6
        WHERE
            EXISTS (
                -- local sender
                SELECT 1 FROM user_account WHERE id = $2
            )
            AND NOT EXISTS (
                -- local recipient
                SELECT 1 FROM user_account WHERE id = $3
            )
        RETURNING invoice
        ",
        &[
            &invoice_id,
            &sender_id,
            &recipient_id,
            &DbChainId::new(chain_id),
            &db_amount,
            &InvoiceStatus::Requested,
        ],
    ).await.map_err(catch_unique_violation("invoice"))?;
    let invoice = row.try_get("invoice")?;
    Ok(invoice)
}

pub async fn get_invoice_by_id(
    db_client: &impl DatabaseClient,
    invoice_id: Uuid,
) -> Result<Invoice, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        SELECT invoice
        FROM invoice WHERE id = $1
        ",
        &[&invoice_id],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("invoice"))?;
    let invoice = row.try_get("invoice")?;
    Ok(invoice)
}

pub async fn get_local_invoice_by_address(
    db_client: &impl DatabaseClient,
    chain_id: &ChainId,
    payment_address: &str,
) -> Result<Invoice, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        SELECT invoice
        FROM invoice
        JOIN user_account ON (invoice.recipient_id = user_account.id)
        WHERE chain_id = $1 AND payment_address = $2
        ",
        &[&DbChainId::new(chain_id), &payment_address],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("invoice"))?;
    let invoice = row.try_get("invoice")?;
    Ok(invoice)
}

pub async fn get_invoice_by_participants(
    db_client: &impl DatabaseClient,
    sender_id: Uuid,
    recipient_id: Uuid,
    chain_id: &ChainId,
) -> Result<Invoice, DatabaseError> {
    // Always return oldest invoice
    let maybe_row = db_client.query_opt(
        "
        SELECT invoice
        FROM invoice
        WHERE
            sender_id = $1
            AND recipient_id = $2
            AND chain_id = $3
        ORDER BY created_at DESC
        LIMIT 1
        ",
        &[
            &sender_id,
            &recipient_id,
            &DbChainId::new(chain_id),
        ],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("invoice"))?;
    let invoice = row.try_get("invoice")?;
    Ok(invoice)
}

pub async fn get_remote_invoice_by_object_id(
    db_client: &impl DatabaseClient,
    object_id: &str,
) -> Result<Invoice, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        SELECT invoice
        FROM invoice
        WHERE object_id = $1
        ",
        &[&object_id],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("invoice"))?;
    let invoice = row.try_get("invoice")?;
    Ok(invoice)
}

pub async fn get_local_invoices_by_status(
    db_client: &impl DatabaseClient,
    chain_id: &ChainId,
    status: InvoiceStatus,
) -> Result<Vec<Invoice>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT invoice
        FROM invoice
        WHERE
            chain_id = $1
            AND invoice_status = $2
            AND object_id IS NULL
        ",
        &[&DbChainId::new(chain_id), &status],
    ).await?;
    let invoices = rows.iter()
        .map(|row| row.try_get("invoice"))
        .collect::<Result<_, _>>()?;
    Ok(invoices)
}

pub async fn get_remote_invoices_by_status(
    db_client: &impl DatabaseClient,
    status: InvoiceStatus,
) -> Result<Vec<Invoice>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT invoice
        FROM invoice
        WHERE invoice_status = $1 AND object_id IS NOT NULL
        ",
        &[&status],
    ).await?;
    let invoices = rows.iter()
        .map(|row| row.try_get("invoice"))
        .collect::<Result<_, _>>()?;
    Ok(invoices)
}

pub async fn set_invoice_status(
    db_client: &mut impl DatabaseClient,
    invoice_id: Uuid,
    new_status: InvoiceStatus,
) -> Result<Invoice, DatabaseError> {
    let transaction = db_client.transaction().await?;
    let maybe_row = transaction.query_opt(
        "
        SELECT invoice
        FROM invoice WHERE id = $1
        FOR UPDATE
        ",
        &[&invoice_id],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("invoice"))?;
    let invoice: Invoice = row.try_get("invoice")?;
    if !invoice.can_change_status(new_status) {
        return Err(DatabaseTypeError.into());
    };
    let maybe_row = transaction.query_opt(
        "
        UPDATE invoice
        SET
            invoice_status = $2,
            updated_at = CURRENT_TIMESTAMP
        WHERE id = $1
        RETURNING invoice
        ",
        &[&invoice_id, &new_status],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("invoice"))?;
    let invoice: Invoice = row.try_get("invoice")?;
    invoice.check_consistency()?;
    transaction.commit().await?;
    Ok(invoice)
}

pub(super) async fn set_invoice_payout_tx_id(
    db_client: &impl DatabaseClient,
    invoice_id: Uuid,
    payout_tx_id: Option<&str>,
) -> Result<Invoice, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        UPDATE invoice SET payout_tx_id = $2
        WHERE id = $1
        RETURNING invoice
        ",
        &[&invoice_id, &payout_tx_id],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("invoice"))?;
    let invoice = row.try_get("invoice")?;
    Ok(invoice)
}

pub(super) async fn set_remote_invoice_data(
    db_client: &impl DatabaseClient,
    invoice_id: Uuid,
    payment_address: &str,
    object_id: &str,
) -> Result<(), DatabaseError> {
    let updated_count = db_client.execute(
        "
        UPDATE invoice
        SET payment_address = $1, object_id = $2
        WHERE id = $3
        RETURNING invoice
        ",
        &[&payment_address, &object_id, &invoice_id],
    ).await?;
    if updated_count == 0 {
        return Err(DatabaseError::NotFound("invoice"));
    };
    Ok(())
}

pub async fn get_invoice_summary(
    db_client: &impl DatabaseClient,
) -> Result<HashMap<InvoiceStatus, i64>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT invoice_status, count(invoice)
        FROM invoice
        GROUP BY invoice_status
        ",
        &[],
    ).await?;
    let summary = rows
        .into_iter()
        .map(|row| {
            let status: InvoiceStatus = row.try_get("invoice_status")?;
            let count: i64 = row.try_get("count")?;
            Ok((status, count))
        })
        .collect::<Result<_, DatabaseError>>()?;
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use serial_test::serial;
    use crate::{
        database::test_utils::create_test_database,
        profiles::test_utils::create_test_remote_profile,
        users::test_utils::create_test_user,
    };
    use super::*;

    async fn create_participants(
        db_client: &mut impl DatabaseClient,
    ) -> (Uuid, Uuid) {
        let local = create_test_user(db_client, "local").await;
        let remote = create_test_remote_profile(
            db_client,
            "remote",
            "social.example",
            "https://social.example/actors/1",
        ).await;
        (local.id, remote.id)
    }

    #[tokio::test]
    #[serial]
    async fn test_create_local_invoice() {
        let db_client = &mut create_test_database().await;
        let (recipient_id, sender_id) =
            create_participants(db_client).await;
        let chain_id = ChainId::monero_mainnet();
        let payment_address = "8MxABajuo71BZya9";
        let amount = 100000000000109212;
        let invoice = create_local_invoice(
            db_client,
            sender_id,
            recipient_id,
            &chain_id,
            payment_address,
            amount,
        ).await.unwrap();
        assert_eq!(invoice.sender_id, sender_id);
        assert_eq!(invoice.recipient_id, recipient_id);
        assert_eq!(invoice.chain_id.into_inner(), chain_id);
        assert_eq!(invoice.amount, amount as i64);
        assert_eq!(invoice.invoice_status, InvoiceStatus::Open);
        assert_eq!(invoice.payment_address.unwrap(), payment_address);
        assert_eq!(invoice.payout_tx_id, None);
        assert_eq!(invoice.updated_at, invoice.created_at);
    }

    #[tokio::test]
    #[serial]
    async fn test_create_remote_invoice() {
        let db_client = &mut create_test_database().await;
        let (sender_id, recipient_id) =
            create_participants(db_client).await;
        let chain_id = ChainId::monero_mainnet();
        let amount = 100000000000109212;
        let invoice = create_remote_invoice(
            db_client,
            sender_id,
            recipient_id,
            &chain_id,
            amount,
        ).await.unwrap();
        assert_eq!(invoice.sender_id, sender_id);
        assert_eq!(invoice.recipient_id, recipient_id);
        assert_eq!(invoice.chain_id.into_inner(), chain_id);
        assert_eq!(invoice.amount, amount as i64);
        assert_eq!(invoice.invoice_status, InvoiceStatus::Requested);
        assert_eq!(invoice.payment_address, None);
        assert_eq!(invoice.payout_tx_id, None);
    }

    #[tokio::test]
    #[serial]
    async fn test_set_invoice_status() {
        let db_client = &mut create_test_database().await;
        let (recipient_id, sender_id) =
            create_participants(db_client).await;
        let invoice = create_local_invoice(
            db_client,
            sender_id,
            recipient_id,
            &ChainId::monero_mainnet(),
            "8MxABajuo71BZya9",
            100000000000000_u64,
        ).await.unwrap();

        let invoice = set_invoice_status(
            db_client,
            invoice.id,
            InvoiceStatus::Paid,
        ).await.unwrap();
        assert_eq!(invoice.invoice_status, InvoiceStatus::Paid);
        assert_ne!(invoice.updated_at, invoice.created_at);

        let error = set_invoice_status(
            db_client,
            invoice.id,
            InvoiceStatus::Cancelled,
        ).await.err().unwrap();
        assert!(matches!(error, DatabaseError::TypeError(_)));
    }
}
