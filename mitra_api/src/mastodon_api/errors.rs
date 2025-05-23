use std::error::{Error as StdError};
use std::time::Duration;

use actix_web::{
    error::ResponseError,
    http::StatusCode,
    HttpResponse,
    HttpResponseBuilder,
};
use serde::Serialize;
use thiserror::Error;

use mitra_models::database::DatabaseError;
use mitra_validators::errors::ValidationError;

#[derive(Debug, Error)]
pub enum MastodonError {
    #[error("database error: {0}")]
    DatabaseError(#[source] DatabaseError),

    #[error("{0}")]
    ValidationError(String),

    #[error("{0}")]
    AuthError(&'static str),

    #[error("permission error")]
    PermissionError,

    #[error("{0} not found")]
    NotFoundError(&'static str),

    #[error("operation not supported")]
    NotSupported,

    #[error("{0}")]
    OperationError(&'static str),

    #[error("retry in {0:.2?}")]
    RateLimit(Duration),

    #[error("internal error: {0}")]
    InternalError(String),
}

impl MastodonError {
    pub fn from_internal(error: impl StdError) -> Self {
        Self::InternalError(error.to_string())
    }

    fn error_message(&self) -> String {
        match self {
            // Don't expose internal error details
            MastodonError::DatabaseError(_) => "database error".to_owned(),
            MastodonError::InternalError(_) => "internal error".to_owned(),
            other_error => other_error.to_string(),
        }
    }
}

impl From<DatabaseError> for MastodonError {
    fn from(error: DatabaseError) -> Self {
        match error {
            DatabaseError::NotFound(name) => Self::NotFoundError(name),
            DatabaseError::AlreadyExists(name) => Self::ValidationError(
                format!("{} already exists", name),
            ),
            _ => Self::DatabaseError(error),
        }
    }
}

impl From<ValidationError> for MastodonError {
    fn from(error: ValidationError) -> Self {
        Self::ValidationError(error.0.to_string())
    }
}

/// https://docs.joinmastodon.org/entities/Error/
#[derive(Serialize)]
pub struct MastodonErrorData {
    error: String,
    error_description: Option<String>,
}

impl MastodonErrorData {
    pub fn new(message: &str) -> Self {
        Self {
            error: message.to_string(),
            error_description: Some(message.to_string()),
        }
    }
}

impl ResponseError for MastodonError {
    fn error_response(&self) -> HttpResponse {
        let error_message = self.error_message();
        let error_data = MastodonErrorData {
            error: error_message.clone(),
            error_description: Some(error_message),
        };
        HttpResponseBuilder::new(self.status_code()).json(error_data)
    }

    fn status_code(&self) -> StatusCode {
        match self {
            Self::ValidationError(_) => StatusCode::BAD_REQUEST,
            Self::AuthError(_) => StatusCode::UNAUTHORIZED,
            Self::PermissionError => StatusCode::FORBIDDEN,
            Self::NotFoundError(_) => StatusCode::NOT_FOUND,
            Self::NotSupported => StatusCode::IM_A_TEAPOT,
            Self::OperationError(_) => StatusCode::UNPROCESSABLE_ENTITY,
            Self::RateLimit(_) => StatusCode::TOO_MANY_REQUESTS,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_message() {
        let db_error = DatabaseError::type_error();
        let error = MastodonError::from(db_error);
        assert_eq!(error.to_string(), "database error: database type error");
        assert_eq!(error.error_message(), "database error");
    }
}
