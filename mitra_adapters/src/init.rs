use std::fs::remove_file;

use log::Level;

use apx_core::{
    crypto_eddsa::{
        ed25519_secret_key_from_bytes,
        generate_ed25519_key,
        Ed25519SecretKey,
    },
    crypto_rsa::{
        generate_rsa_key,
        rsa_secret_key_from_pkcs1_der,
        rsa_secret_key_to_pkcs1_der,
        RsaSecretKey,
    },
};
use mitra_config::{
    parse_config,
    Config,
    SOFTWARE_NAME,
    SOFTWARE_VERSION,
};
use mitra_models::{
    database::{
        connect,
        migrate::apply_migrations,
        utils::get_postgres_version,
        BasicDatabaseClient,
        DatabaseClient,
        DatabaseConnectionPool,
        DatabaseError,
        DatabaseTypeError,
    },
    properties::constants::{
        INSTANCE_ED25519_SECRET_KEY,
        INSTANCE_RSA_SECRET_KEY,
    },
    properties::queries::{
        get_internal_property,
        set_internal_property,
    },
    users::helpers::add_ed25519_keys,
};
use mitra_services::media::MediaStorage;

use crate::logger::configure_logger;

pub fn initialize_app(
    override_log_level: Option<Level>,
) -> Config {
    let (config, config_warnings) = parse_config();
    let log_level = override_log_level.unwrap_or(config.log_level);
    configure_logger(log_level);
    log::info!(
        "{} v{}, environment = '{:?}'",
        SOFTWARE_NAME,
        SOFTWARE_VERSION,
        config.environment,
    );
    log::info!("config loaded from {}", config.config_path);
    for warning in config_warnings {
        log::warn!("{}", warning);
    };
    config
}

// Panics on errors
pub async fn create_database_client(config: &Config) -> BasicDatabaseClient {
    connect::create_database_client(
        &config.database_url,
        config.database_tls_ca_file.as_deref(),
    ).await.expect("failed to connect to database")
}

// Panics on errors
pub fn create_database_connection_pool(config: &Config)
    -> DatabaseConnectionPool
{
    // https://wiki.postgresql.org/wiki/Number_Of_Database_Connections
    // https://docs.rs/deadpool/0.10.0/src/deadpool/managed/config.rs.html#54
    let db_pool_size = num_cpus::get_physical() * 2;
    log::info!("database connection pool size: {db_pool_size}");
    let db_pool = connect::create_database_connection_pool(
        &config.database_url,
        config.database_tls_ca_file.as_deref(),
        db_pool_size,
    ).expect("failed to connect to database");
    db_pool
}

async fn check_postgres_version(
    db_client: &impl DatabaseClient,
) -> Result<(), DatabaseError> {
    let version = get_postgres_version(db_client).await?;
    if version < 130_000 {
        log::error!("unsupported PostgreSQL version: {version}");
    } else {
        log::info!("PostgreSQL version: {version}");
    };
    Ok(())
}

async fn apply_custom_migrations(
    db_client: &impl DatabaseClient,
) -> Result<(), DatabaseError> {
    // TODO: remove migration
    let updated_count = add_ed25519_keys(db_client).await?;
    if updated_count > 0 {
        log::info!("generated ed25519 keys for {updated_count} users");
    };
    Ok(())
}

async fn save_instance_rsa_key(
    db_client: &impl DatabaseClient,
    secret_key: &RsaSecretKey,
) -> Result<(), DatabaseError> {
    let secret_key_der = rsa_secret_key_to_pkcs1_der(secret_key)
        .map_err(|_| DatabaseTypeError)?;
    set_internal_property(
        db_client,
        INSTANCE_RSA_SECRET_KEY,
        &secret_key_der,
    ).await?;
    Ok(())
}

async fn prepare_instance_rsa_key(
    db_client: &impl DatabaseClient,
) -> Result<RsaSecretKey, DatabaseError> {
    let maybe_secret_key_bytes: Option<Vec<u8>> =
        get_internal_property(db_client, INSTANCE_RSA_SECRET_KEY)
            .await?;
    let secret_key = if let Some(secret_key_der) = maybe_secret_key_bytes {
        rsa_secret_key_from_pkcs1_der(&secret_key_der)
            .map_err(|_| DatabaseTypeError)?
    } else {
        let secret_key = generate_rsa_key()
            .expect("RSA key generation should succeed");
        save_instance_rsa_key(db_client, &secret_key).await?;
        log::info!("instance RSA key generated");
        secret_key
    };
    Ok(secret_key)
}

async fn prepare_instance_ed25519_key(
    db_client: &impl DatabaseClient,
) -> Result<Ed25519SecretKey, DatabaseError> {
    let maybe_secret_key_bytes: Option<Vec<u8>> =
        get_internal_property(db_client, INSTANCE_ED25519_SECRET_KEY)
            .await?;
    let secret_key = if let Some(secret_key_bytes) = maybe_secret_key_bytes {
        ed25519_secret_key_from_bytes(&secret_key_bytes)
            .map_err(|_| DatabaseTypeError)?
    } else {
        let secret_key = generate_ed25519_key();
        set_internal_property(
            db_client,
            INSTANCE_ED25519_SECRET_KEY,
            &secret_key,
        ).await?;
        log::info!("instance Ed25519 key generated");
        secret_key
    };
    Ok(secret_key)
}

async fn prepare_instance_keys(
    config: &mut Config,
    db_client: &impl DatabaseClient,
) -> Result<(), DatabaseError> {
    if let Some(instance_rsa_key) = config.get_instance_rsa_key() {
        save_instance_rsa_key(db_client, instance_rsa_key).await?;
        log::warn!("instance RSA key copied from file");
        let secret_key_path = config.storage_dir.join("instance_rsa_key");
        remove_file(secret_key_path)
            .expect("can't delete instance_rsa_key file");
        log::warn!("instance_rsa_key file deleted");
    } else {
        let instance_rsa_key = prepare_instance_rsa_key(db_client).await?;
        config.set_instance_rsa_key(instance_rsa_key);
    };
    let instance_ed25519_key = prepare_instance_ed25519_key(db_client).await?;
    config.set_instance_ed25519_key(instance_ed25519_key);
    Ok(())
}

// Panics on errors
pub async fn initialize_database(
    config: &mut Config,
    db_client: &mut BasicDatabaseClient,
) -> () {
    check_postgres_version(db_client).await
        .expect("failed to verify PostgreSQL version");
    apply_migrations(db_client).await
        .expect("failed to apply migrations");
    apply_custom_migrations(db_client).await
        .expect("failed to apply custom migrations");
    prepare_instance_keys(config, db_client).await
        .expect("failed to prepare instance keys");
}

// Panics on errors
pub fn initialize_storage(
    config: &Config,
) -> () {
    let media_storage = MediaStorage::new(config);
    match media_storage {
        MediaStorage::Filesystem(ref backend) => {
            backend.init().expect("failed to create media directory");
        },
    };
}

#[cfg(test)]
mod tests {
    use serial_test::serial;
    use mitra_models::database::test_utils::create_test_database;
    use super::*;

    #[tokio::test]
    #[serial]
    async fn test_prepare_instance_ed25519_key() {
        let db_client = &create_test_database().await;
        let key_1 = prepare_instance_ed25519_key(db_client).await.unwrap();
        let key_2 = prepare_instance_ed25519_key(db_client).await.unwrap();
        assert_eq!(key_1, key_2);
    }
}
