use anyhow::Error;
use clap::Parser;
use uuid::Uuid;

use mitra_adapters::payments::monero::invoice_payment_address;
use mitra_config::Config;
use mitra_models::{
    database::DatabaseClient,
    invoices::{
        helpers::local_invoice_forwarded,
        queries::get_invoice_by_id,
        types::InvoiceStatus,
    },
};
use mitra_services::monero::wallet::{
    get_outgoing_transfers,
    get_subaddress_index,
    open_monero_wallet,
};

/// Repair invoice after a forwarding error
#[derive(Parser)]
pub struct RepairInvoice {
    invoice_id: Uuid,
}

impl RepairInvoice {
    pub async fn execute(
        &self,
        config: &Config,
        db_client: &mut impl DatabaseClient,
    ) -> Result<(), Error> {
        let monero_config = config.monero_config()
            .ok_or(Error::msg("monero configuration not found"))?;
        let wallet_client = open_monero_wallet(monero_config).await?;
        let invoice = get_invoice_by_id(db_client, self.invoice_id).await?;
        if invoice.invoice_status != InvoiceStatus::Paid {
            return Err(Error::msg("invoice is not paid"));
        };
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
