use apx_sdk::fetch::FetchError;
use serde_json::{Error as DeserializationError};
use thiserror::Error;

use mitra_models::database::DatabaseError;
use mitra_services::media::MediaStorageError;
use mitra_validators::errors::ValidationError;

#[derive(Debug, Error)]
pub enum HandlerError {
    #[error("local object")]
    LocalObject,

    #[error(transparent)]
    FetchError(#[from] FetchError),

    #[error("{0}")]
    ValidationError(String),

    #[error(transparent)]
    DatabaseError(#[from] DatabaseError),

    #[error("media storage error")]
    StorageError(#[from] MediaStorageError),

    #[error("{0}")]
    ServiceError(&'static str),

    #[error("{0}")]
    Filtered(String),
}

impl From<DeserializationError> for HandlerError {
    fn from(error: DeserializationError) -> Self {
        Self::ValidationError(format!("deserialization error: {error}"))
    }
}

impl From<ValidationError> for HandlerError {
    fn from(error: ValidationError) -> Self {
        Self::ValidationError(error.to_string())
    }
}
