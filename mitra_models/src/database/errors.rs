use thiserror::Error;

#[derive(Debug, Error)]
#[error("database type error")]
pub struct DatabaseTypeError;

#[derive(Debug, Error)]
pub enum DatabaseError {
    #[error("database pool error")]
    PoolError(#[from] deadpool_postgres::PoolError),

    #[error("database query error")]
    QueryError(#[from] postgres_query::Error),

    #[error(transparent)]
    ClientError(#[from] tokio_postgres::Error),

    #[error(transparent)]
    TypeError(#[from] DatabaseTypeError),

    #[error("{0} not found")]
    NotFound(&'static str), // object type

    #[error("{0} already exists")]
    AlreadyExists(&'static str), // object type
}

impl DatabaseError {
    pub fn type_error() -> Self {
        Self::from(DatabaseTypeError)
    }
}
