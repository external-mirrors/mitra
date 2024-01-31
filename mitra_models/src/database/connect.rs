use std::path::Path;

use deadpool_postgres::Pool;
use openssl::ssl::{SslConnector, SslMethod};
use postgres_openssl::MakeTlsConnector;
use tokio_postgres::config::{Config as DatabaseConfig};

fn create_tls_connector(ca_file_path: &Path) -> MakeTlsConnector {
    let mut builder = SslConnector::builder(SslMethod::tls()).unwrap();
    log::info!("using TLS CA file: {}", ca_file_path.display());
    builder.set_ca_file(ca_file_path).unwrap();
    MakeTlsConnector::new(builder.build())
}

pub async fn create_database_client(
    db_config: &DatabaseConfig,
    ca_file_path: Option<&Path>,
) -> tokio_postgres::Client {
    if let Some(ca_file_path) = ca_file_path {
        let connector = create_tls_connector(ca_file_path);
        let (client, connection) = db_config.connect(connector).await.unwrap();
        tokio::spawn(async move {
            if let Err(err) = connection.await {
                log::error!("connection with tls error: {}", err);
            };
        });

        client
    } else {
        let (client, connection) = db_config.connect(tokio_postgres::NoTls).await.unwrap();
        tokio::spawn(async move {
            if let Err(err) = connection.await {
                log::error!("connection error: {}", err);
            };
        });

        client
    }
}

pub fn create_pool(
    database_url: &str,
    ca_file_path: Option<&Path>,
    pool_size: usize,
) -> Pool {
    let database_config = database_url.parse().expect("invalid database URL");
    let manager = if let Some(ca_file_path) = ca_file_path {
        let connector = create_tls_connector(ca_file_path);
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

    Pool::builder(manager)
        .max_size(pool_size)
        .build()
        .unwrap()
}
