use mitra_config::{parse_config, Config};
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    users::helpers::add_ed25519_keys,
};

use crate::logger::configure_logger;

pub fn initialize_app() -> Config {
    let (config, config_warnings) = parse_config();
    configure_logger(config.log_level);
    log::info!("config loaded from {}", config.config_path);
    for warning in config_warnings {
        log::warn!("{}", warning);
    };
    #[cfg(target_env = "musl")]
    {
        let is_found = openssl_probe::try_init_ssl_cert_env_vars();
        if !is_found {
            log::error!("certificate store not found");
        };
    };
    config
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
