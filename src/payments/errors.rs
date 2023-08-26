use mitra_models::database::DatabaseError;

use crate::ethereum::EthereumError;
use crate::monero::wallet::MoneroError;

#[derive(thiserror::Error, Debug)]
pub enum PaymentError {
    #[error(transparent)]
    EthereumError(#[from] EthereumError),

    #[error(transparent)]
    MoneroError(#[from] MoneroError),

    #[error(transparent)]
    DatabaseError(#[from] DatabaseError),
}
