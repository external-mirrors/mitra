use chrono::Utc;

use mitra_models::{
    database::{
        get_database_client,
        DatabaseConnectionPool,
        DatabaseError,
    },
    invoices::{
        queries::{
            get_remote_invoices_by_status,
            set_invoice_status,
        },
        types::InvoiceStatus,
    },
};

use super::monero::MONERO_INVOICE_TIMEOUT;

// Assume remote server is similar to ours
// TODO: check `endTime` on `Agreement`
pub const REMOTE_INVOICE_TIMEOUT: u32 = MONERO_INVOICE_TIMEOUT;

pub async fn check_open_remote_invoices(
    db_pool: &DatabaseConnectionPool,
) -> Result<(), DatabaseError> {
    let db_client = &mut **get_database_client(db_pool).await?;
    let open_invoices = get_remote_invoices_by_status(
        db_client,
        InvoiceStatus::Open,
    ).await?;
    for invoice in open_invoices {
        let expires_at = invoice.expires_at(REMOTE_INVOICE_TIMEOUT);
        if expires_at <= Utc::now() {
            log::info!("invoice {}: timed out", invoice.id);
            set_invoice_status(
                db_client,
                invoice.id,
                InvoiceStatus::Timeout,
            ).await?;
        };
    };
    Ok(())
}
