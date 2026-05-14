use anyhow::{anyhow, Error};
use clap::Parser;
use uuid::Uuid;

use mitra_adapters::payments::monero::{
    get_payment_address,
    invoice_payment_address,
    reopen_local_invoice,
};
use mitra_config::Config;
use mitra_models::{
    database::{get_database_client, DatabaseConnectionPool},
    invoices::{
        helpers::{
            get_local_invoice_by_id,
            local_invoice_forwarded,
        },
        types::InvoiceStatus,
    },
    payment_methods::types::PaymentType,
};
use mitra_services::monero::wallet::{
    get_outgoing_transfers,
    get_subaddress_index,
    open_monero_wallet,
};

/// Re-open closed invoice (already processed, timed out or cancelled)
#[derive(Parser)]
pub struct ReopenInvoice {
    id: Uuid,
}

impl ReopenInvoice {
    pub async fn execute(
        self,
        config: &Config,
        db_pool: &DatabaseConnectionPool,
    ) -> Result<(), Error> {
        let db_client = &mut **get_database_client(db_pool).await?;
        let monero_config = config.monero_config()
            .ok_or(anyhow!("monero integration is not enabled"))?;
        let invoice = get_local_invoice_by_id(
            db_client,
            PaymentType::Monero,
            &monero_config.chain_id,
            self.id,
        ).await?;
        reopen_local_invoice(
            monero_config,
            db_client,
            &invoice,
        ).await?;
        Ok(())
    }
}

/// Repair invoice after a forwarding error
#[derive(Parser)]
pub struct RepairInvoice {
    invoice_id: Uuid,
}

impl RepairInvoice {
    pub async fn execute(
        self,
        config: &Config,
        db_pool: &DatabaseConnectionPool,
    ) -> Result<(), Error> {
        let db_client = &mut **get_database_client(db_pool).await?;
        let monero_config = config.monero_config()
            .ok_or(Error::msg("monero integration is not enabled"))?;
        let invoice = get_local_invoice_by_id(
            db_client,
            PaymentType::Monero,
            &monero_config.chain_id,
            self.invoice_id,
        ).await?;
        if invoice.invoice_status != InvoiceStatus::Paid {
            return Err(Error::msg("invoice is not paid"));
        };
        let wallet_client = open_monero_wallet(monero_config).await?;
        let payment_address = invoice_payment_address(&invoice)?;
        let address_index = get_subaddress_index(
            &wallet_client,
            monero_config.account_index,
            &payment_address,
        ).await?;
        let outgoing_transfers = get_outgoing_transfers(
            &wallet_client,
            monero_config.account_index,
            address_index.minor,
        ).await?;
        let payout_tx_id = match &outgoing_transfers[..] {
            [] => return Err(Error::msg("no outgoing transfers")),
            [transfer] => format!("{}", transfer.txid),
            _ => return Err(Error::msg("multiple outgoing transfers")),
        };
        println!("found outgoing transfer: {}", payout_tx_id);
        local_invoice_forwarded(
            db_client,
            invoice.id,
            &payout_tx_id,
        ).await?;
        println!("invoice updated");
        Ok(())
    }
}

/// Get payment address for given sender and recipient
#[derive(Parser)]
pub struct GetPaymentAddress {
    sender_id: Uuid,
    /// Local recipient
    recipient_id: Uuid,
}

impl GetPaymentAddress {
    pub async fn execute(
        self,
        config: &Config,
        db_pool: &DatabaseConnectionPool,
    ) -> Result<(), Error> {
        let db_client = &mut **get_database_client(db_pool).await?;
        let monero_config = config.monero_config()
            .ok_or(anyhow!("monero integration is not enabled"))?;
        let payment_address = get_payment_address(
            monero_config,
            db_client,
            self.sender_id,
            self.recipient_id,
        ).await?;
        println!("payment address: {}", payment_address);
        Ok(())
    }
}
