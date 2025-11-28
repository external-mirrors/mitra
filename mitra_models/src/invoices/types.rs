use apx_core::caip2::ChainId;
use chrono::{DateTime, TimeDelta, Utc};
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

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum InvoiceStatus {
    Open,
    Paid,
    Forwarded,
    Timeout,
    Cancelled,
    Underpaid,
    Completed,
    Failed,
    Requested,
}

impl InvoiceStatus {
    pub fn is_final(self) -> bool {
        matches!(
            self,
            Self::Timeout |
            Self::Cancelled |
            Self::Underpaid |
            Self::Completed |
            Self::Failed)
    }
}

impl From<InvoiceStatus> for i16 {
    fn from(value: InvoiceStatus) -> i16 {
        match value {
            InvoiceStatus::Open => 1,
            InvoiceStatus::Paid => 2,
            InvoiceStatus::Forwarded => 3,
            InvoiceStatus::Timeout => 4,
            InvoiceStatus::Cancelled => 5,
            InvoiceStatus::Underpaid => 6,
            InvoiceStatus::Completed => 7,
            InvoiceStatus::Failed => 8,
            InvoiceStatus::Requested => 9,
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
            8 => Self::Failed,
            9 => Self::Requested,
            _ => return Err(DatabaseTypeError),
        };
        Ok(invoice_status)
    }
}

int_enum_from_sql!(InvoiceStatus);
int_enum_to_sql!(InvoiceStatus);

#[derive(FromSql)]
#[postgres(name = "invoice")]
pub struct Invoice {
    pub id: Uuid,
    pub sender_id: Uuid,
    pub recipient_id: Uuid,
    pub chain_id: DbChainId,
    pub amount: i64, // requested payment amount
    pub invoice_status: InvoiceStatus,
    pub payment_address: Option<String>,
    pub payout_tx_id: Option<String>,
    pub object_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Invoice {
    pub(super) fn check_consistency(&self) -> Result<(), DatabaseTypeError> {
        if self.object_id.is_none() {
            // Local invoice
            if !self.chain_id.inner().is_monero() {
                return Err(DatabaseTypeError);
            };
            if self.payment_address.is_none() {
                return Err(DatabaseTypeError);
            };
        } else {
            // Remote invoice
            if self.payout_tx_id.is_some() {
                return Err(DatabaseTypeError);
            };
            if self.payment_address.is_none()
                && self.invoice_status != InvoiceStatus::Requested
            {
                return Err(DatabaseTypeError);
            };
        };
        Ok(())
    }

    pub fn amount_u64(&self) -> Result<u64, DatabaseTypeError> {
        u64::try_from(self.amount).map_err(|_| DatabaseTypeError)
    }

    pub fn can_change_status(&self, to: InvoiceStatus) -> bool {
        use InvoiceStatus::*;
        let allowed = match self.invoice_status {
            Open => {
                if self.object_id.is_some() {
                    vec![Paid, Completed, Timeout, Cancelled]
                } else {
                    vec![Paid, Timeout, Cancelled]
                }
            },
            Paid => {
                if self.object_id.is_some() {
                    vec![Completed]
                } else if self.payout_tx_id.is_some() {
                    vec![Forwarded, Underpaid]
                } else {
                    vec![Underpaid]
                }
            },
            Forwarded => vec![Completed, Failed],
            Timeout => {
                if self.object_id.is_some() {
                    vec![Completed]
                } else {
                    vec![Paid]
                }
            },
            Cancelled => vec![Paid],
            Underpaid => vec![Paid],
            Completed => {
                if self.object_id.is_some() {
                    vec![]
                } else {
                    vec![Paid]
                }
            },
            Failed => vec![Paid],
            Requested => {
                if self.payment_address.is_some() && self.object_id.is_some() {
                    vec![Open, Cancelled]
                } else {
                    vec![Cancelled]
                }
            }
        };
        allowed.contains(&to)
    }

    pub fn try_payment_address(&self) -> Result<String, DatabaseTypeError> {
        match self.invoice_status {
            InvoiceStatus::Requested => panic!("payment address is not known"),
            _ => self.payment_address.clone().ok_or(DatabaseTypeError),
        }
    }

    pub fn expires_at(&self, timeout: u32) -> DateTime<Utc> {
        self.created_at + TimeDelta::seconds(timeout.into())
    }
}

#[cfg(feature = "test-utils")]
impl Default for Invoice {
    fn default() -> Self {
        Self {
            id: Default::default(),
            sender_id: Default::default(),
            recipient_id: Default::default(),
            chain_id: DbChainId(ChainId::monero_mainnet()),
            amount: 1,
            invoice_status: InvoiceStatus::Open,
            payment_address: Some("".to_string()),
            payout_tx_id: None,
            object_id: None,
            created_at: Default::default(),
            updated_at: Default::default(),
        }
    }
}
