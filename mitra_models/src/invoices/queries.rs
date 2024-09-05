use uuid::Uuid;

use apx_core::caip2::ChainId;
use mitra_utils::id::generate_ulid;

use crate::database::{
    catch_unique_violation,
    DatabaseClient,
    DatabaseError,
    DatabaseTypeError,
};

use super::types::{DbChainId, DbInvoice, InvoiceStatus};

/// Create invoice with local recipient
pub async fn create_local_invoice(
    db_client: &impl DatabaseClient,
    sender_id: &Uuid,
    recipient_id: &Uuid,
    chain_id: &ChainId,
    payment_address: &str,
    amount: impl TryInto<i64>,
) -> Result<DbInvoice, DatabaseError> {
    let invoice_id = generate_ulid();
    let db_amount: i64 = TryInto::try_into(amount)
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
    sender_id: &Uuid,
    recipient_id: &Uuid,
    chain_id: &ChainId,
    amount: impl TryInto<i64>,
) -> Result<DbInvoice, DatabaseError> {
    let invoice_id = generate_ulid();
    let db_amount: i64 = TryInto::try_into(amount)
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
    invoice_id: &Uuid,
) -> Result<DbInvoice, DatabaseError> {
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
) -> Result<DbInvoice, DatabaseError> {
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
    sender_id: &Uuid,
    recipient_id: &Uuid,
    chain_id: &ChainId,
) -> Result<DbInvoice, DatabaseError> {
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
) -> Result<DbInvoice, DatabaseError> {
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

pub async fn get_invoices_by_status(
    db_client: &impl DatabaseClient,
    chain_id: &ChainId,
    status: InvoiceStatus,
    only_local: bool,
) -> Result<Vec<DbInvoice>, DatabaseError> {
    let condition = if only_local { "AND object_id IS NULL" } else { "" };
    let statement = format!(
        "
        SELECT invoice
        FROM invoice
        JOIN actor_profile ON (invoice.recipient_id = actor_profile.id)
        WHERE
            chain_id = $1
            AND invoice_status = $2
            {condition}
        ",
    );
    let rows = db_client.query(
        &statement,
        &[&DbChainId::new(chain_id), &status],
    ).await?;
    let invoices = rows.iter()
        .map(|row| row.try_get("invoice"))
        .collect::<Result<_, _>>()?;
    Ok(invoices)
}

pub async fn get_local_invoices_by_status(
    db_client: &impl DatabaseClient,
    chain_id: &ChainId,
    status: InvoiceStatus,
) -> Result<Vec<DbInvoice>, DatabaseError> {
    get_invoices_by_status(db_client, chain_id, status, true).await
}

pub async fn set_invoice_status(
    db_client: &mut impl DatabaseClient,
    invoice_id: &Uuid,
    new_status: InvoiceStatus,
) -> Result<DbInvoice, DatabaseError> {
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
    let invoice: DbInvoice = row.try_get("invoice")?;
    if !invoice.can_change_status(&new_status) {
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
    let invoice = row.try_get("invoice")?;
    transaction.commit().await?;
    Ok(invoice)
}

pub(super) async fn set_invoice_payout_tx_id(
    db_client: &impl DatabaseClient,
    invoice_id: &Uuid,
    payout_tx_id: Option<&str>,
) -> Result<DbInvoice, DatabaseError> {
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
    invoice_id: &Uuid,
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

#[cfg(test)]
mod tests {
    use serial_test::serial;
    use crate::database::test_utils::create_test_database;
    use crate::profiles::{
        queries::create_profile,
        types::ProfileCreateData,
    };
    use crate::users::{
        queries::create_user,
        types::UserCreateData,
    };
    use super::*;

    async fn create_participants(
        db_client: &mut impl DatabaseClient,
    ) -> (Uuid, Uuid) {
        let user_data = UserCreateData {
            username: "local".to_string(),
            password_hash: Some("test".to_string()),
            ..Default::default()
        };
        let user = create_user(db_client, user_data).await.unwrap();
        let profile_data = ProfileCreateData {
            username: "remote".to_string(),
            ..Default::default()
        };
        let profile = create_profile(db_client, profile_data).await.unwrap();
        (user.id, profile.id)
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
            &sender_id,
            &recipient_id,
            &chain_id,
            payment_address,
            amount,
        ).await.unwrap();
        assert_eq!(invoice.sender_id, sender_id);
        assert_eq!(invoice.recipient_id, recipient_id);
        assert_eq!(invoice.chain_id.into_inner(), chain_id);
        assert_eq!(invoice.amount, amount);
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
            &sender_id,
            &recipient_id,
            &chain_id,
            amount,
        ).await.unwrap();
        assert_eq!(invoice.sender_id, sender_id);
        assert_eq!(invoice.recipient_id, recipient_id);
        assert_eq!(invoice.chain_id.into_inner(), chain_id);
        assert_eq!(invoice.amount, amount);
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
            &sender_id,
            &recipient_id,
            &ChainId::monero_mainnet(),
            "8MxABajuo71BZya9",
            100000000000000_u64,
        ).await.unwrap();

        let invoice = set_invoice_status(
            db_client,
            &invoice.id,
            InvoiceStatus::Paid,
        ).await.unwrap();
        assert_eq!(invoice.invoice_status, InvoiceStatus::Paid);
        assert_ne!(invoice.updated_at, invoice.created_at);

        let error = set_invoice_status(
            db_client,
            &invoice.id,
            InvoiceStatus::Cancelled,
        ).await.err().unwrap();
        assert!(matches!(error, DatabaseError::DatabaseTypeError(_)));
    }
}
