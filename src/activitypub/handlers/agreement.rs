use serde::Deserialize;

use crate::activitypub::valueflows::parsers::Quantity;

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
    pub clauses: (Commitment, Commitment),
    pub url: Option<PaymentLink>,
}
