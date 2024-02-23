use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::str::FromStr;

use mitra_utils::{
    crypto_rsa::{
        rsa_private_key_from_pkcs8_pem,
        RsaPrivateKey,
    },
};

use super::blockchain::BlockchainConfig;
use super::config::Config;
use super::environment::Environment;

struct EnvConfig {
    config_path: String,
    environment: Option<Environment>,
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
        .map(|val| Environment::from_str(&val).expect("invalid environment type"));
    EnvConfig {
        config_path,
        environment,
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
fn read_instance_rsa_key(storage_dir: &Path) -> Option<RsaPrivateKey> {
    let private_key_path = storage_dir.join("instance_rsa_key");
    if private_key_path.exists() {
        let private_key_str = std::fs::read_to_string(&private_key_path)
            .expect("failed to read instance RSA key");
        let private_key = rsa_private_key_from_pkcs8_pem(&private_key_str)
            .expect("failed to read instance RSA key");
        Some(private_key)
    } else {
        None
    }
}

pub fn parse_config() -> (Config, Vec<&'static str>) {
    let env = parse_env();
    let config_yaml = std::fs::read_to_string(&env.config_path)
        .expect("failed to load config file");
    let mut config = serde_yaml::from_str::<Config>(&config_yaml)
        .expect("invalid yaml data");
    let mut warnings = vec![];

    // Set parameters from environment
    config.config_path = env.config_path;
    if let Some(environment) = env.environment {
        // Overwrite default only if ENVIRONMENT variable is set
        config.environment = environment;
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
    if config.blockchains().len() > 1 {
        warnings.push("multichain deployments are not recommended");
    };
    for blockchain_config in config.blockchains() {
        match blockchain_config {
            BlockchainConfig::Ethereum(ethereum_config) => {
                ethereum_config.try_ethereum_chain_id()
                    .expect("invalid ethereum chain ID");
                if !ethereum_config.contract_dir.exists() {
                    panic!("contract directory does not exist");
                };
            },
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
    config.instance_rsa_key = read_instance_rsa_key(&config.storage_dir);

    (config, warnings)
}
