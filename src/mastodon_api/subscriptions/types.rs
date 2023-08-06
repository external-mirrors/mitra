use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use mitra_models::{
    invoices::types::{DbInvoice, InvoiceStatus},
    profiles::types::PaymentOption,
};
use mitra_utils::caip2::ChainId;

use crate::monero::subscriptions::MONERO_INVOICE_TIMEOUT;

#[derive(Deserialize)]
pub struct InvoiceData {
    pub sender_id: Uuid,
    pub recipient_id: Uuid,
    pub chain_id: ChainId,
    pub amount: i64,
}

#[derive(Serialize)]
pub struct Invoice {
    pub id: Uuid,
    pub sender_id: Uuid,
    pub recipient_id: Uuid,
    pub chain_id: ChainId,
    pub payment_address: String,
    pub amount: i64,
    pub status: String,
    pub expires_at: DateTime<Utc>,
}

impl From<DbInvoice> for Invoice {
    fn from(value: DbInvoice) -> Self {
        let status = match value.invoice_status {
            InvoiceStatus::Open => "open",
            InvoiceStatus::Paid => "paid",
            InvoiceStatus::Forwarded => "forwarded",
            InvoiceStatus::Timeout => "timeout",
            InvoiceStatus::Cancelled => "cancelled",
            InvoiceStatus::Underpaid => "underpaid",
            InvoiceStatus::Completed => "completed",
            InvoiceStatus::Failed => "failed",
        };
        let expires_at = if value.chain_id.inner().is_monero() {
            value.created_at + Duration::seconds(MONERO_INVOICE_TIMEOUT)
        } else {
            // Epoch 0
            Default::default()
        };
        Self {
            id: value.id,
            sender_id: value.sender_id,
            recipient_id: value.recipient_id,
            chain_id: value.chain_id.into_inner(),
            payment_address: value.payment_address,
            amount: value.amount,
            status: status.to_string(),
            expires_at,
        }
    }
}

#[derive(Deserialize)]
pub struct SubscriptionAuthorizationQueryParams {
    pub price: u64,
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum SubscriptionOption {
    Ethereum {
        chain_id: ChainId,
    },
    Monero {
        chain_id: ChainId,
        price: u64,
        payout_address: String,
    },
}

impl SubscriptionOption {
    pub fn from_payment_option(payment_option: PaymentOption) -> Option<Self> {
        let settings = match payment_option {
            PaymentOption::Link(_) => return None,
            PaymentOption::EthereumSubscription(payment_info) => Self::Ethereum {
                chain_id: payment_info.chain_id,
            },
            PaymentOption::MoneroSubscription(payment_info) => Self::Monero {
                chain_id: payment_info.chain_id,
                price: payment_info.price.into(),
                payout_address: payment_info.payout_address,
            },
            PaymentOption::RemoteMoneroSubscription(_) => return None,
        };
        Some(settings)
    }
}

#[derive(Deserialize)]
pub struct SubscriptionQueryParams {
    pub sender_id: Uuid,
    pub recipient_id: Uuid,
}

#[derive(Serialize)]
pub struct SubscriptionDetails {
    pub id: i32,
    pub expires_at: DateTime<Utc>,
}
