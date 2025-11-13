pub mod connect;
pub mod errors;
pub mod int_enum;
pub mod json_macro;
pub mod migrate;
pub mod query_macro;
pub mod utils;

#[cfg(any(test, feature = "test-utils"))]
pub mod test_utils;

pub type DatabaseConnectionPool = deadpool_postgres::Pool;
pub use tokio_postgres::{GenericClient as DatabaseClient};
pub use tokio_postgres::{Client as BasicDatabaseClient};
pub use errors::{
    catch_unique_violation,
    DatabaseError,
    DatabaseTypeError,
};

pub async fn get_database_client(
    db_pool: &DatabaseConnectionPool,
) -> Result<deadpool_postgres::Client, DatabaseError> {
    // Returns wrapped client
    // https://github.com/bikeshedder/deadpool/issues/56
    let client = db_pool.get().await?;
    Ok(client)
}

// WARNING: must be used with caution in `match` (or `if let`) scrutenees
// because the temporary (connection) will not be dropped until
// the end of the whole `match` expression.
// https://github.com/rust-lang/rust/issues/93883
#[macro_export]
macro_rules! db_client_await {
    ($pool: expr) => {
        &**get_database_client($pool).await?
    };
}

pub use db_client_await;
