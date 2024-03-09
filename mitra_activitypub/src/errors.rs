use thiserror::Error;

use mitra_federation::fetch::FetchError;
use mitra_models::database::DatabaseError;
use mitra_services::media::MediaStorageError;
use mitra_validators::errors::ValidationError;

#[derive(Debug, Error)]
pub enum HandlerError {
    #[error("local object")]
    LocalObject,

    #[error(transparent)]
    FetchError(#[from] FetchError),

    #[error(transparent)]
    ValidationError(#[from] ValidationError),

    #[error(transparent)]
    DatabaseError(#[from] DatabaseError),

    #[error("media storage error")]
    StorageError(#[from] MediaStorageError),

    #[error("{0}")]
    ServiceError(&'static str),

    #[error("unsolicited message from {0}")]
    UnsolicitedMessage(String),
}
