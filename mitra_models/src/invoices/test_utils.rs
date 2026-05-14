use apx_core::caip2::ChainId;

use crate::{
    database::DatabaseClient,
    payment_methods::types::PaymentType,
    profiles::test_utils::create_test_local_profile,
    users::test_utils::create_test_user,
};

use super::{
    queries::create_local_invoice,
    types::{DbChainId, Invoice, InvoiceStatus},
};

impl Default for Invoice {
    fn default() -> Self {
        Self {
            id: Default::default(),
            sender_id: Default::default(),
            recipient_id: Default::default(),
            chain_id: DbChainId::new(&ChainId::monero_mainnet()),
            amount: 1,
            invoice_status: InvoiceStatus::Open,
            payment_type: Some(PaymentType::Monero),
            payment_address: Some("".to_string()),
            payout_tx_id: None,
            payout_amount: None,
            object_id: None,
            created_at: Default::default(),
            updated_at: Default::default(),
        }
    }
}

pub async fn create_test_local_invoice(
    db_client: &mut impl DatabaseClient,
) -> Invoice {
    let sender = create_test_local_profile(
        db_client,
        "sender",
    ).await;
    let recipient = create_test_user(db_client, "recipient").await;
    let invoice = create_local_invoice(
        db_client,
        sender.id,
        recipient.id,
        PaymentType::Monero,
        &ChainId::monero_mainnet(),
        "8MxABajuo71BZya9",
        100000000000000_u64,
    ).await.unwrap();
    invoice.check_consistency().unwrap();
    invoice
}
