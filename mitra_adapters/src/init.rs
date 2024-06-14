use mitra_config::{parse_config, Config};
use mitra_models::{
    database::{
        DatabaseClient,
        DatabaseError,
        DatabaseTypeError,
        utils::get_postgres_version,
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
use mitra_utils::{
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

use crate::logger::configure_logger;

pub fn initialize_app() -> Config {
    let (config, config_warnings) = parse_config();
    configure_logger(config.log_level);
    log::info!("config loaded from {}", config.config_path);
    for warning in config_warnings {
        log::warn!("{}", warning);
    };
    #[cfg(all(feature = "native-tls", target_env = "musl"))]
    {
        let is_found = openssl_probe::try_init_ssl_cert_env_vars();
        if !is_found {
            log::error!("certificate store not found");
        };
    };
    config
}

pub async fn check_postgres_version(
    db_client: &impl  DatabaseClient,
) -> () {
    if let Ok(version) = get_postgres_version(db_client).await {
        if version < 130_000 {
            log::warn!("unsupported PostgreSQL version: {version}");
        } else {
            log::info!("PostgreSQL version: {version}");
        };
    };
}

pub async fn apply_custom_migrations(
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

pub async fn prepare_instance_keys(
    config: &mut Config,
    db_client: &impl DatabaseClient,
) -> Result<(), DatabaseError> {
    if let Some(instance_rsa_key) = config.get_instance_rsa_key() {
        save_instance_rsa_key(db_client, instance_rsa_key).await?;
        log::info!("instance RSA key copied from file");
    } else {
        let instance_rsa_key = prepare_instance_rsa_key(db_client).await?;
        config.set_instance_rsa_key(instance_rsa_key);
    };
    let instance_ed25519_key = prepare_instance_ed25519_key(db_client).await?;
    config.set_instance_ed25519_key(instance_ed25519_key);
    Ok(())
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
