use std::time::Duration;

use actix_web::{
    error::ResponseError,
    http::StatusCode,
    HttpResponse,
    HttpResponseBuilder,
};
use serde::Serialize;

use mitra_models::database::DatabaseError;
use mitra_validators::errors::ValidationError;

#[derive(thiserror::Error, Debug)]
pub enum MastodonError {
    #[error("database error")]
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

    #[error("internal error")]
    InternalError,
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
        let error_data = MastodonErrorData {
            error: self.to_string(),
            error_description: Some(self.to_string()),
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
