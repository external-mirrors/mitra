use thiserror::Error;

use mitra_activitypub::{
    authentication::AuthenticationError,
    errors::HandlerError,
};
use mitra_models::database::errors::DatabaseError;
use mitra_validators::errors::ValidationError;

use crate::errors::HttpError;

#[derive(Debug, Error)]
pub enum EndpointError {
    #[error(transparent)]
    ValidationError(#[from] ValidationError),

    #[error(transparent)]
    DatabaseError(#[from] DatabaseError),

    #[error("authentication error: {0}")]
    AuthError(#[source] AuthenticationError),
}

impl From<AuthenticationError> for EndpointError {
    fn from(error: AuthenticationError) -> Self {
        match error {
            AuthenticationError::ValidationError(inner) => inner.into(),
            AuthenticationError::DatabaseError(inner) => inner.into(),
            _ => Self::AuthError(error),
        }
    }
}

impl From<EndpointError> for HttpError {
    fn from(error: EndpointError) -> Self {
        match error {
            EndpointError::ValidationError(error) => error.into(),
            EndpointError::DatabaseError(error) => error.into(),
            EndpointError::AuthError(_) => {
                HttpError::AuthError("signature verification error")
            },
        }
    }
}

impl From<HandlerError> for HttpError {
    fn from(error: HandlerError) -> Self {
         match error {
            HandlerError::ValidationError(error) =>
                HttpError::ValidationError(error),
            HandlerError::DatabaseError(error) => error.into(),
            other_error => HttpError::from_internal(other_error),
        }
    }
}
