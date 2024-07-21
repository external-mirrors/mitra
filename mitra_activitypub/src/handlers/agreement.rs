use serde::Deserialize;

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
#[serde(rename_all = "camelCase")]
pub struct Agreement {
    pub id: Option<String>,

    pub stipulates: Commitment,
    pub stipulates_reciprocal: Commitment,

    pub url: Option<PaymentLink>,
}

impl Agreement {
    pub fn primary_commitment(&self) -> &Commitment {
        &self.stipulates
    }

    pub fn reciprocal_commitment(&self) -> &Commitment {
        &self.stipulates_reciprocal
    }
}
