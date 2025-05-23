use std::error::{Error as StdError};

use actix_web::{
    error::ResponseError,
    http::StatusCode,
    HttpResponse,
    HttpResponseBuilder,
};
use thiserror::Error;

use mitra_models::database::DatabaseError;
use mitra_validators::errors::ValidationError;

#[derive(Debug, Error)]
pub enum HttpError {
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

    #[error("internal error: {0}")]
    InternalError(String),
}

impl HttpError {
    pub fn from_internal(error: impl StdError) -> Self {
        Self::InternalError(error.to_string())
    }

    fn error_message(&self) -> String {
        match self {
            // Don't expose internal error details
            HttpError::DatabaseError(_) => "database error".to_owned(),
            HttpError::InternalError(_) => "internal error".to_owned(),
            other_error => other_error.to_string(),
        }
    }
}

impl From<DatabaseError> for HttpError {
    fn from(err: DatabaseError) -> Self {
        match err {
            DatabaseError::NotFound(name) => HttpError::NotFoundError(name),
            DatabaseError::AlreadyExists(name) => HttpError::ValidationError(
                format!("{} already exists", name),
            ),
            _ => HttpError::DatabaseError(err),
        }
    }
}

impl From<ValidationError> for HttpError {
    fn from(error: ValidationError) -> Self {
        Self::ValidationError(error.0.to_string())
    }
}

impl ResponseError for HttpError {
    fn error_response(&self) -> HttpResponse {
        let error_message = self.error_message();
        HttpResponseBuilder::new(self.status_code()).body(error_message)
    }

    fn status_code(&self) -> StatusCode {
        match self {
            HttpError::ValidationError(_) => StatusCode::BAD_REQUEST,
            HttpError::AuthError(_) => StatusCode::UNAUTHORIZED,
            HttpError::PermissionError => StatusCode::FORBIDDEN,
            HttpError::NotFoundError(_) => StatusCode::NOT_FOUND,
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
        let error = HttpError::from(db_error);
        assert_eq!(error.to_string(), "database error: database type error");
        assert_eq!(error.error_message(), "database error");
    }
}
