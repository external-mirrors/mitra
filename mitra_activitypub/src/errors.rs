use apx_sdk::fetch::FetchError;
use serde_json::{Error as DeserializationError};
use thiserror::Error;

use mitra_models::database::DatabaseError;
use mitra_services::media::MediaStorageError;
use mitra_validators::errors::ValidationError;

use crate::authentication::AuthenticationError;

#[derive(Debug, Error)]
pub enum HandlerError {
    #[error("local object: {0}")]
    LocalObject(String),

    #[error(transparent)]
    FetchError(#[from] FetchError),

    #[error("{0}")]
    ValidationError(String),

    #[error(transparent)]
    DatabaseError(#[from] DatabaseError),

    #[error("media storage error: {0}")]
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

impl From<AuthenticationError> for HandlerError {
    fn from(error: AuthenticationError) -> Self {
        match error {
            AuthenticationError::DatabaseError(db_error) => db_error.into(),
            _ => {
                // HTTP signatures are not verified in handlers
                let error_message = format!("invalid integrity proof: {error}");
                Self::ValidationError(error_message)
            },
        }
    }
}
