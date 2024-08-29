use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::str::FromStr;

use mitra_utils::{
    crypto_rsa::{
        rsa_secret_key_from_pkcs8_pem,
        RsaSecretKey,
    },
};

use super::blockchain::BlockchainConfig;
use super::config::Config;
use super::environment::Environment;

struct EnvConfig {
    config_path: String,
    environment: Environment,
    http_port: Option<u32>,
}

#[cfg(feature = "production")]
const DEFAULT_CONFIG_PATH: &str = "/etc/mitra/config.yaml";
#[cfg(not(feature = "production"))]
const DEFAULT_CONFIG_PATH: &str = "config.yaml";

fn parse_env() -> EnvConfig {
    dotenvy::from_filename(".env.local").ok();
    dotenvy::dotenv().ok();
    let config_path = std::env::var("CONFIG_PATH")
        .unwrap_or(DEFAULT_CONFIG_PATH.to_string());
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

/// Read secret key from instance_rsa_key file
fn read_instance_rsa_key(storage_dir: &Path) -> Option<RsaSecretKey> {
    let secret_key_path = storage_dir.join("instance_rsa_key");
    if secret_key_path.exists() {
        let secret_key_str = std::fs::read_to_string(&secret_key_path)
            .expect("failed to read instance RSA key");
        let secret_key = rsa_secret_key_from_pkcs8_pem(&secret_key_str)
            .expect("failed to read instance RSA key");
        Some(secret_key)
    } else {
        None
    }
}

pub fn parse_config() -> (Config, Vec<&'static str>) {
    let env = parse_env();
    let config_yaml = std::fs::read_to_string(&env.config_path)
        .unwrap_or_else(|_| {
            panic!("failed to read config from {}", env.config_path);
        });
    let mut config = serde_yaml::from_str::<Config>(&config_yaml)
        .expect("invalid yaml data");
    let mut warnings = vec![];

    // Set parameters from environment
    config.config_path = env.config_path;
    config.environment = env.environment;
    if let Some(http_port) = env.http_port {
        config.http_port = http_port;
    };

    // Validate config
    if !config.storage_dir.exists() {
        panic!("storage directory does not exist");
    };
    check_directory_owner(&config.storage_dir);
    config.try_instance_url().expect("invalid instance URI");
    if config.authentication_methods.is_empty() {
        panic!("authentication_methods must not be empty");
    };
    if !config.federation.fep_1b12_full_enabled {
        warnings.push("federation.fep_1b12_full_enabled parameter is deprecated");
    };
    if config.blockchains().len() > 1 {
        warnings.push("multichain deployments are not recommended");
    };
    for blockchain_config in config.blockchains() {
        match blockchain_config {
            BlockchainConfig::Monero(monero_config) => {
                monero_config.chain_id.monero_network()
                    .expect("invalid monero chain ID");
            },
        };
    };
    if config.ipfs_api_url.is_some() != config.ipfs_gateway_url.is_some() {
        panic!("both ipfs_api_url and ipfs_gateway_url must be set");
    };

    // Insert instance RSA key
    if let Some(instance_rsa_key) = read_instance_rsa_key(&config.storage_dir) {
        config.instance_rsa_key = Some(instance_rsa_key);
        warnings.push("instance_rsa_key file can be deleted");
    };

    (config, warnings)
}
