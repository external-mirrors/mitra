use chrono::{DateTime, Utc};
use postgres_protocol::types::{text_from_sql, text_to_sql};
use postgres_types::{
    accepts,
    private::BytesMut,
    to_sql_checked,
    FromSql,
    IsNull,
    ToSql,
    Type,
};
use uuid::Uuid;

use mitra_utils::caip2::ChainId;

use crate::database::{
    int_enum::{int_enum_from_sql, int_enum_to_sql},
    DatabaseTypeError,
};

#[derive(Debug)]
pub struct DbChainId(ChainId);

impl DbChainId {
    pub fn new(chain_id: &ChainId) -> Self {
        Self(chain_id.clone())
    }

    pub fn inner(&self) -> &ChainId {
        let Self(chain_id) = self;
        chain_id
    }

    pub fn into_inner(self) -> ChainId {
        let Self(chain_id) = self;
        chain_id
    }
}

impl PartialEq<ChainId> for DbChainId {
    fn eq(&self, other: &ChainId) -> bool {
        self.inner() == other
    }
}

impl<'a> FromSql<'a> for DbChainId {
    fn from_sql(
        _: &Type,
        raw: &'a [u8],
    ) -> Result<Self, Box<dyn std::error::Error + Sync + Send>> {
        let value_str = text_from_sql(raw)?;
        let value: ChainId = value_str.parse()?;
        Ok(Self(value))
    }

    accepts!(VARCHAR);
}

impl ToSql for DbChainId {
    fn to_sql(
        &self,
        _: &Type,
        out: &mut BytesMut,
    ) -> Result<IsNull, Box<dyn std::error::Error + Sync + Send>> {
        let value_str = self.inner().to_string();
        text_to_sql(&value_str, out);
        Ok(IsNull::No)
    }

    accepts!(VARCHAR, TEXT);
    to_sql_checked!();
}

#[derive(Debug, PartialEq)]
pub enum InvoiceStatus {
    Open,
    Paid,
    Forwarded,
    Timeout,
    Cancelled,
    Underpaid,
    Completed,
}

impl InvoiceStatus {
    pub fn can_change(&self, to: &Self) -> bool {
        let allowed = match self {
            Self::Open => vec![Self::Paid, Self::Timeout, Self::Cancelled],
            Self::Paid => vec![Self::Forwarded, Self::Underpaid],
            Self::Forwarded => vec![Self::Completed],
            Self::Timeout => vec![Self::Paid],
            Self::Cancelled => vec![Self::Paid],
            Self::Underpaid => vec![Self::Paid],
            Self::Completed => vec![Self::Paid],
        };
        allowed.contains(to)
    }

    pub fn is_final(&self) -> bool {
        matches!(
            self,
            Self::Timeout |
            Self::Cancelled |
            Self::Underpaid |
            Self::Completed)
    }
}

impl From<&InvoiceStatus> for i16 {
    fn from(value: &InvoiceStatus) -> i16 {
        match value {
            InvoiceStatus::Open => 1,
            InvoiceStatus::Paid => 2,
            InvoiceStatus::Forwarded => 3,
            InvoiceStatus::Timeout => 4,
            InvoiceStatus::Cancelled => 5,
            InvoiceStatus::Underpaid => 6,
            InvoiceStatus::Completed => 7,
        }
    }
}

impl TryFrom<i16> for InvoiceStatus {
    type Error = DatabaseTypeError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        let invoice_status = match value {
            1 => Self::Open,
            2 => Self::Paid,
            3 => Self::Forwarded,
            4 => Self::Timeout,
            5 => Self::Cancelled,
            6 => Self::Underpaid,
            7 => Self::Completed,
            _ => return Err(DatabaseTypeError),
        };
        Ok(invoice_status)
    }
}

int_enum_from_sql!(InvoiceStatus);
int_enum_to_sql!(InvoiceStatus);

#[derive(FromSql)]
#[postgres(name = "invoice")]
pub struct DbInvoice {
    pub id: Uuid,
    pub sender_id: Uuid,
    pub recipient_id: Uuid,
    pub chain_id: DbChainId,
    pub payment_address: String,
    pub amount: i64, // requested payment amount
    pub invoice_status: InvoiceStatus,
    pub payout_tx_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
