use mitra_models::database::DatabaseError;
use crate::monero::wallet::MoneroError;

#[derive(thiserror::Error, Debug)]
pub enum PaymentError {
    #[error(transparent)]
    MoneroError(#[from] MoneroError),

    #[error(transparent)]
    DatabaseError(#[from] DatabaseError),
}
