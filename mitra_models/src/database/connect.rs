use std::path::Path;

use deadpool_postgres::Pool;
use tokio_postgres::{
    config::{Config as DatabaseConfig},
    Client,
};

#[cfg(feature = "native-tls")]
use {
    openssl::{
        error::{ErrorStack as OpenSslError},
        ssl::{SslConnector, SslMethod},
    },
    postgres_openssl::MakeTlsConnector,
};

#[cfg(feature = "rustls-tls")]
use {
    rustls::{
        client::ClientConfig as TlsClientConfig,
        Certificate,
        Error as RustlsError,
        RootCertStore,
    },
    rustls_pemfile::certs,
    tokio_postgres_rustls::MakeRustlsConnect,
};

#[derive(thiserror::Error, Debug)]
pub enum DatabaseConnectionError {
    #[error(transparent)]
    CertificateError(#[from] std::io::Error),

    #[error("{0}")]
    TlsError(String),

    #[error(transparent)]
    PostgresError(#[from] tokio_postgres::Error),

    #[error(transparent)]
    PoolError(#[from] deadpool::managed::BuildError<tokio_postgres::Error>),
}

#[cfg(feature = "native-tls")]
impl From<OpenSslError> for DatabaseConnectionError {
    fn from(error: OpenSslError) -> Self {
        Self::TlsError(error.to_string())
    }
}

#[cfg(feature = "rustls-tls")]
impl From<RustlsError> for DatabaseConnectionError {
    fn from(error: RustlsError) -> Self {
        Self::TlsError(error.to_string())
    }
}

#[cfg(feature = "native-tls")]
fn create_tls_connector(
    ca_file_path: &Path,
) -> Result<MakeTlsConnector, DatabaseConnectionError> {
    let mut builder = SslConnector::builder(SslMethod::tls())?;
    log::info!("using TLS CA file: {}", ca_file_path.display());
    builder.set_ca_file(ca_file_path)?;
    let connector = MakeTlsConnector::new(builder.build());
    Ok(connector)
}

#[cfg(feature = "rustls-tls")]
fn create_tls_connector(
    ca_file_path: &Path,
) -> Result<MakeRustlsConnect, DatabaseConnectionError> {
    let mut root_store = RootCertStore::empty();
    let ca_file = std::fs::File::open(ca_file_path)?;
    let mut ca_file_reader = std::io::BufReader::new(ca_file);
    for item in certs(&mut ca_file_reader)? {
        root_store.add(&Certificate(item))?;
    };
    let tls_config = TlsClientConfig::builder()
        .with_safe_defaults()
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
