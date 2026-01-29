use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::str::FromStr;

use super::blockchain::{
    BlockchainConfig,
    MoneroConfig,
    MoneroLightConfig,
};
use super::config::Config;
use super::environment::Environment;
use super::instance::{
    is_correct_uri_scheme,
    parse_instance_url,
};

const DEFAULT_CONFIG_PATH: &str = "config.yaml";

fn default_config_path() -> &'static str {
    let maybe_path = option_env!("DEFAULT_CONFIG_PATH");
    maybe_path.unwrap_or(DEFAULT_CONFIG_PATH)
}

struct EnvConfig {
    config_path: String,
    environment: Environment,
    http_port: Option<u32>,
}

fn parse_env() -> EnvConfig {
    dotenvy::from_filename(".env.local").ok();
    dotenvy::dotenv().ok();
    let config_path = std::env::var("CONFIG_PATH")
        .unwrap_or(default_config_path().to_string());
    let environment = std::env::var("ENVIRONMENT").ok()
        .map(|val| Environment::from_str(&val).expect("invalid environment type"))
        // Default depends on "production" feature flag
        .unwrap_or_default();
    let maybe_http_port = std::env::var("HTTP_PORT").ok()
        .map(|val| u32::from_str(&val).expect("invalid port number"));
    EnvConfig {
        config_path,
        environment,
        http_port: maybe_http_port,
    }
}

extern "C" {
    fn geteuid() -> u32;
}

fn check_directory_owner(path: &Path) -> () {
    let metadata = std::fs::metadata(path)
        .expect("can't read file metadata");
    let owner_uid = metadata.uid();
    let current_uid = unsafe { geteuid() };
    if owner_uid != current_uid {
        panic!(
            "{} owner ({}) is different from the current user ({})",
            path.display(),
            owner_uid,
            current_uid,
        );
    };
}

pub fn parse_config() -> (Config, Vec<&'static str>) {
    let env = parse_env();
    let config_text = std::fs::read_to_string(&env.config_path)
        .unwrap_or_else(|_| {
            panic!("failed to read config from {}", env.config_path);
        });
    let mut config: Config = if env.config_path.ends_with(".toml") {
        toml::from_str(&config_text)
            .expect("invalid TOML config file")
    } else {
        serde_yaml::from_str(&config_text)
            .expect("invalid YAML config file")
    };
    let mut warnings = vec![];

    // Set parameters from environment
    config.config_path = env.config_path;
    config.environment = env.environment;
    if let Some(http_port) = env.http_port {
        config.http_port = Some(http_port);
    };

    // Validate config
    if !config.storage_dir.exists() {
        panic!("storage directory does not exist");
    };
    check_directory_owner(&config.storage_dir);
    if let Some(ref web_client_dir) = config.web_client_dir {
        if !web_client_dir.exists() {
            panic!(
                "web client directory does not exist: {}",
                web_client_dir.display(),
            );
        };
    };
    config.http_socket();
    #[cfg(feature = "mini")]
    let instance_url = &config.gateway_url;
    #[cfg(not(feature = "mini"))]
    let instance_url = &config.instance_url;

    let instance_uri = parse_instance_url(instance_url)
        .expect("invalid instance URL");
    if !is_correct_uri_scheme(&instance_uri) {
        warnings.push("instance_url may have incorrect URL scheme");
    };
    if config.authentication_methods.is_empty() {
        panic!("authentication_methods must not be empty");
    };
    if !config.federation.ssrf_protection_enabled {
        warnings.push("SSRF protection disabled");
    };
    if !config.federation.fep_1b12_full_enabled {
        warnings.push("federation.fep_1b12_full_enabled parameter is deprecated");
    };
    if config.blocked_instances.is_some() {
        warnings.push("blocked_instances parameter is deprecated (use `mitra add-filter-rule`)");
    };
    if config.allowed_instances.is_some() {
        warnings.push("allowed_instances parameter is deprecated (use `mitra add-filter-rule`)");
    };
    for blockchain_config in config.blockchains() {
        match blockchain_config {
            BlockchainConfig::Monero(MoneroConfig { chain_id, .. }) |
                BlockchainConfig::MoneroLight(MoneroLightConfig { chain_id, .. }) =>
            {
                chain_id.monero_network()
                    .expect("invalid monero chain ID");
            },
        };
    };
    if config.ipfs_api_url.is_some() != config.ipfs_gateway_url.is_some() {
        panic!("both ipfs_api_url and ipfs_gateway_url must be set");
    };

    (config, warnings)
}
