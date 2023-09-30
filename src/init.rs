use mitra_config::{parse_config, Config};

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
        openssl_probe::init_ssl_cert_env_vars();
    };
    config
}
