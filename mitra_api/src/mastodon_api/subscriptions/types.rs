use apx_core::caip2::ChainId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use mitra_adapters::payments::{
    common::REMOTE_INVOICE_TIMEOUT,
    monero::MONERO_INVOICE_TIMEOUT,
};
use mitra_models::{
    invoices::types::{Invoice as DbInvoice, InvoiceStatus},
    profiles::types::PaymentOption,
    subscriptions::types::{Subscription as DbSubscription},
};

use crate::mastodon_api::serializers::serialize_datetime;

#[derive(Deserialize)]
pub struct SubscriberData {
    pub subscriber_id: Uuid,
    pub duration: i32,
}

#[derive(Deserialize)]
pub struct InvoiceData {
    pub sender_id: Uuid,
    pub recipient_id: Uuid,
    pub chain_id: ChainId,
    pub amount: u64,
}

#[derive(Serialize)]
pub struct Invoice {
    pub id: Uuid,
    pub sender_id: Uuid,
    pub recipient_id: Uuid,
    pub chain_id: ChainId,
    pub payment_address: Option<String>,
    pub amount: i64,
    pub status: String,
    #[serde(serialize_with = "serialize_datetime")]
    pub created_at: DateTime<Utc>,
    #[serde(serialize_with = "serialize_datetime")]
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
            InvoiceStatus::Requested => "requested",
        };
        let expires_at = if value.object_id.is_some() {
            // TODO: remote servers should specify payment window
            value.expires_at(REMOTE_INVOICE_TIMEOUT)
        } else if value.chain_id.inner().is_monero() {
            // Supported chain
            // Invoice will be displayed as active
            // even if integration is disabled.
            value.expires_at(MONERO_INVOICE_TIMEOUT)
        } else {
            // Unsupported chain
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
            created_at: value.created_at,
            expires_at,
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum SubscriptionOption {
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
    #[serde(serialize_with = "serialize_datetime")]
    pub expires_at: DateTime<Utc>,
}

impl From<DbSubscription> for SubscriptionDetails {
    fn from(db_subscription: DbSubscription) -> Self {
        Self {
            id: db_subscription.id,
            expires_at: db_subscription.expires_at,
        }
    }
}
