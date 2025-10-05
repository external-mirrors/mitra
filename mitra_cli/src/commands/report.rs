use anyhow::Error;
use clap::Parser;

use mitra_config::Config;
use mitra_models::{
    background_jobs::queries::get_job_count,
    background_jobs::types::JobType,
    database::{get_database_client, DatabaseConnectionPool},
    invoices::{
        queries::get_invoice_summary,
        types::InvoiceStatus,
    },
    posts::queries::get_post_count,
    subscriptions::queries::{
        get_active_subscription_count,
        get_expired_subscription_count,
    },
    users::queries::get_user_count,
};
use mitra_services::{
    monero::{
        wallet::{
            get_address_count,
            open_monero_wallet,
        },
    },
};

/// Display instance report
#[derive(Parser)]
pub struct InstanceReport;

impl InstanceReport {
    pub async fn execute(
        &self,
        config: &Config,
        db_pool: &DatabaseConnectionPool,
    ) -> Result<(), Error> {
        let db_client = &**get_database_client(db_pool).await?;
        // General info
        let users = get_user_count(db_client).await?;
        let posts = get_post_count(db_client, false).await?;
        println!("local users: {users}");
        println!("total posts: {posts}");
        // Queues
        let incoming_activities =
            get_job_count(db_client, JobType::IncomingActivity).await?;
        let outgoing_activities =
            get_job_count(db_client, JobType::OutgoingActivity).await?;
        let data_import_queue_size =
            get_job_count(db_client, JobType::DataImport).await?;
        let fetcher_queue_size =
            get_job_count(db_client, JobType::Fetcher).await?;
        println!("incoming activity queue: {incoming_activities}");
        println!("outgoing activity queue: {outgoing_activities}");
        println!("data import queue: {data_import_queue_size}");
        println!("fetcher queue: {fetcher_queue_size}");
        // Invoices
        let invoice_summary = get_invoice_summary(db_client).await?;
        for invoice_status in [
            InvoiceStatus::Open,
            InvoiceStatus::Paid,
            InvoiceStatus::Underpaid,
            InvoiceStatus::Forwarded,
            InvoiceStatus::Failed,
        ] {
            let status_str = format!("{invoice_status:?}").to_lowercase();
            let count = invoice_summary
                .get(&invoice_status)
                .unwrap_or(&0);
            println!("{status_str} invoices: {count}");
        };
        // Subscriptions
        let active_subscriptions =
            get_active_subscription_count(db_client).await?;
        let expired_subscriptions =
            get_expired_subscription_count(db_client).await?;
        println!("active subscriptions: {}", active_subscriptions);
        println!("expired subscriptions: {}", expired_subscriptions);
        if let Some(monero_config) = config.monero_config() {
            let wallet_client = open_monero_wallet(monero_config).await?;
            let address_count = get_address_count(
                &wallet_client,
                monero_config.account_index,
            ).await?;
            println!("monero addresses: {}", address_count);
        };
        Ok(())
    }
}
