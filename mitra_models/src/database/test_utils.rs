use tokio_postgres::Client;
use tokio_postgres::config::Config;

use super::connect::create_database_client_from_config;
use super::migrate::apply_migrations;

const DEFAULT_CONNECTION_URL: &str = "postgres://mitra:mitra@127.0.0.1:55432/mitra-test";

pub async fn create_test_database() -> Client {
    let connection_url = std::env::var("TEST_DATABASE_URL")
        .unwrap_or(DEFAULT_CONNECTION_URL.to_string());
    let mut db_config: Config = connection_url.parse()
        .expect("invalid database connection URL");
    let db_name = db_config.get_dbname()
        .expect("database name not specified")
        .to_string();

    // Create connection without database name
    db_config.dbname("");
    let db_client = create_database_client_from_config(&db_config, None).await
        .expect("should create database client");
    let drop_db_statement = format!(
        "DROP DATABASE IF EXISTS {db_name:?}",
        db_name=db_name,
    );
    db_client.execute(&drop_db_statement, &[]).await.unwrap();
    let create_db_statement = format!(
        "CREATE DATABASE {db_name:?} WITH OWNER={owner:?};",
        db_name=db_name,
        owner=db_config.get_user().unwrap(),
    );
    db_client.execute(&create_db_statement, &[]).await.unwrap();

    // Create new connection to database
    db_config.dbname(&db_name);
    let mut db_client = create_database_client_from_config(&db_config, None).await
        .expect("should create database client");
    apply_migrations(&mut db_client).await
        .expect("failed to apply migrations");
    db_client
}
