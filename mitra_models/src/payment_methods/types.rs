use apx_core::{
    caip2::ChainId,
};
use postgres_types::FromSql;
use uuid::Uuid;

use crate::{
    database::{
        int_enum::{int_enum_from_sql, int_enum_to_sql},
        DatabaseTypeError,
    },
    invoices::types::DbChainId,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PaymentType {
    Monero,
}

impl From<PaymentType> for i16 {
    fn from(value: PaymentType) -> i16 {
        match value {
            PaymentType::Monero => 1,
        }
    }
}

impl TryFrom<i16> for PaymentType {
    type Error = DatabaseTypeError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        let payment_type = match value {
            1 => Self::Monero,
            _ => return Err(DatabaseTypeError),
        };
        Ok(payment_type)
    }
}

int_enum_from_sql!(PaymentType);
int_enum_to_sql!(PaymentType);

pub struct PaymentMethodData {
    pub owner_id: Uuid,
    pub payment_type: PaymentType,
    pub chain_id: ChainId,
    pub payout_address: String,
}

#[derive(FromSql)]
#[postgres(name = "payment_method")]
pub struct PaymentMethod {
    pub id: i32,
    pub owner_id: Uuid,
    pub payment_type: PaymentType,
    pub chain_id: DbChainId,
    pub payout_address: String,
}

impl PaymentMethod {
    pub(super) fn check_consistency(&self) -> Result<(), DatabaseTypeError> {
        match self.payment_type {
            PaymentType::Monero => {
                if !self.chain_id.inner().is_monero() {
                    return Err(DatabaseTypeError);
                };
            },
        };
        Ok(())
    }
}
