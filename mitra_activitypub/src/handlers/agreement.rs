use serde::Deserialize;

use mitra_validators::errors::ValidationError;

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

    // TODO: remove
    pub clauses: Option<(Commitment, Commitment)>,
    // TODO: make required
    pub stipulates: Option<Commitment>,
    pub stipulates_reciprocal: Option<Commitment>,

    pub url: Option<PaymentLink>,
}

impl Agreement {
    pub fn primary_commitment(&self) -> Result<&Commitment, ValidationError> {
        self.stipulates.as_ref()
            .or(self.clauses.as_ref().map(|clauses| &clauses.0))
            .ok_or(ValidationError("primary commitment is missing"))
    }

    pub fn reciprocal_commitment(&self) -> Result<&Commitment, ValidationError> {
        self.stipulates_reciprocal.as_ref()
            .or(self.clauses.as_ref().map(|clauses| &clauses.1))
            .ok_or(ValidationError("reciprocal commitment is missing"))
    }
}
