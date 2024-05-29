use tokio_postgres::error::{Error as PgError, SqlState};

pub mod connect;
pub mod int_enum;
pub mod json_macro;
pub mod migrate;
pub mod query_macro;

#[cfg(any(test, feature = "test-utils"))]
pub mod test_utils;

pub type DatabaseConnectionPool = deadpool_postgres::Pool;
pub use tokio_postgres::{GenericClient as DatabaseClient};

#[derive(thiserror::Error, Debug)]
#[error("database type error")]
pub struct DatabaseTypeError;

#[derive(thiserror::Error, Debug)]
pub enum DatabaseError {
    #[error("database pool error")]
    DatabasePoolError(#[from] deadpool_postgres::PoolError),

    #[error("database query error")]
    DatabaseQueryError(#[from] postgres_query::Error),

    #[error(transparent)]
    DatabaseClientError(#[from] tokio_postgres::Error),

    #[error(transparent)]
    DatabaseTypeError(#[from] DatabaseTypeError),

    #[error("{0} not found")]
    NotFound(&'static str), // object type

    #[error("{0} already exists")]
    AlreadyExists(&'static str), // object type
}

pub async fn get_database_client(
    db_pool: &DatabaseConnectionPool,
) -> Result<deadpool_postgres::Client, DatabaseError> {
    // Returns wrapped client
    // https://github.com/bikeshedder/deadpool/issues/56
    let client = db_pool.get().await?;
    Ok(client)
}

pub fn catch_unique_violation(
    object_type: &'static str,
) -> impl Fn(PgError) -> DatabaseError {
    move |err| {
        if let Some(code) = err.code() {
            if code == &SqlState::UNIQUE_VIOLATION {
                return DatabaseError::AlreadyExists(object_type);
            };
        };
        err.into()
    }
}
