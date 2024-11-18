use std::path::Path;

use deadpool_postgres::Pool;
use rustls::{
    client::ClientConfig as TlsClientConfig,
    Error as RustlsError,
    RootCertStore,
};
use rustls_pemfile::certs;
use tokio_postgres::{
    config::{Config as DatabaseConfig},
    Client,
};
use tokio_postgres_rustls::MakeRustlsConnect;

#[derive(thiserror::Error, Debug)]
pub enum DatabaseConnectionError {
    #[error(transparent)]
    CertificateError(#[from] std::io::Error),

    #[error(transparent)]
    TlsError(#[from] RustlsError),

    #[error(transparent)]
    PostgresError(#[from] tokio_postgres::Error),

    #[error(transparent)]
    PoolError(#[from] deadpool::managed::BuildError<tokio_postgres::Error>),
}

fn create_tls_connector(
    ca_file_path: &Path,
) -> Result<MakeRustlsConnect, DatabaseConnectionError> {
    let mut root_store = RootCertStore::empty();
    let ca_file = std::fs::File::open(ca_file_path)?;
    let mut ca_file_reader = std::io::BufReader::new(ca_file);
    for maybe_item in certs(&mut ca_file_reader) {
        root_store.add(maybe_item?)?;
    };
    let tls_config = TlsClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();
    let connector = MakeRustlsConnect::new(tls_config);
    Ok(connector)
}

pub async fn create_database_client(
    db_config: &DatabaseConfig,
    ca_file_path: Option<&Path>,
) -> Result<Client, DatabaseConnectionError> {
    let client = if let Some(ca_file_path) = ca_file_path {
        let connector = create_tls_connector(ca_file_path)?;
        let (client, connection) = db_config.connect(connector).await?;
        tokio::spawn(async move {
            if let Err(err) = connection.await {
                log::error!("connection with tls error: {}", err);
            };
        });
        client
    } else {
        let (client, connection) = db_config.connect(tokio_postgres::NoTls).await?;
        tokio::spawn(async move {
            if let Err(err) = connection.await {
                log::error!("connection error: {}", err);
            };
        });
        client
    };
    Ok(client)
}

pub fn create_pool(
    database_url: &str,
    ca_file_path: Option<&Path>,
    pool_size: usize,
) -> Result<Pool, DatabaseConnectionError> {
    let database_config = database_url.parse()?;
    let manager = if let Some(ca_file_path) = ca_file_path {
        let connector = create_tls_connector(ca_file_path)?;
        deadpool_postgres::Manager::new(
            database_config,
            connector,
        )
    } else {
        deadpool_postgres::Manager::new(
            database_config,
            tokio_postgres::NoTls,
        )
    };
    let db_pool = Pool::builder(manager)
        .max_size(pool_size)
        .build()?;
    Ok(db_pool)
}
