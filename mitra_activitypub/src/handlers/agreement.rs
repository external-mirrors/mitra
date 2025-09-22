use serde::Deserialize;

use mitra_models::invoices::types::InvoiceStatus;

use super::proposal::Quantity;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Commitment {
    pub satisfies: String,
    pub resource_quantity: Quantity,
}

#[derive(Deserialize)]
pub struct PaymentLink {
    pub href: String,
}

#[derive(Deserialize)]
pub struct PaymentStatus {
    name: String,
}

impl PaymentStatus {
    pub fn invoice_status(&self) -> Option<InvoiceStatus> {
        let options = vec![InvoiceStatus::Paid];
        options.into_iter()
            .find(|status| self.name == format!("{status:?}"))
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Agreement {
    pub id: Option<String>,

    pub stipulates: Commitment,
    pub stipulates_reciprocal: Commitment,

    pub url: Option<PaymentLink>,
    pub preview: Option<PaymentStatus>,
}

impl Agreement {
    pub fn primary_commitment(&self) -> &Commitment {
        &self.stipulates
    }

    pub fn reciprocal_commitment(&self) -> &Commitment {
        &self.stipulates_reciprocal
    }
}
