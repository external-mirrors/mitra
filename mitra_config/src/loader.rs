use std::str::FromStr;

use apx_core::url::hostname::is_same_apex_domain;

use super::{
    blockchain::{
        BlockchainConfig,
        MoneroConfig,
        MoneroLightConfig,
    },
    config::Config,
    environment::Environment,
    instance::{
        is_correct_uri_scheme,
        parse_instance_url,
    },
    software::SoftwareMetadata,
};

const DEFAULT_CONFIG_PATH: &str = "config.yaml";

// Default is set at compile time
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

pub fn parse_config(
    software_metadata: SoftwareMetadata,
) -> (Config, Vec<String>) {
    let env = parse_env();
    let config_text = std::fs::read_to_string(&env.config_path)
        .unwrap_or_else(|_| {
            panic!("failed to read config from {}", env.config_path);
        });
    let mut unused_parameters = vec![];
    let mut config: Config = if env.config_path.ends_with(".toml") {
        let deserializer = toml::Deserializer::parse(&config_text)
            .expect("invalid TOML config file");
        serde_ignored::deserialize(deserializer, |path| {
            unused_parameters.push(path.to_string());
        }).expect("invalid TOML config file")
    } else {
        let deserializer = serde_yaml::Deserializer::from_str(&config_text);
        serde_ignored::deserialize(deserializer, |path| {
            unused_parameters.push(path.to_string());
        }).expect("invalid YAML config file")
    };
    let mut warnings = vec![];
    for parameter in unused_parameters {
        let message = format!("unused configuration parameter: {parameter}");
        warnings.push(message);
    };

    // Set software metadata
    config.software = software_metadata;
    // Set parameters from environment
    config.config_path = env.config_path;
    config.environment = env.environment;
    if let Some(http_port) = env.http_port {
        config.http_port = Some(http_port);
    };

    // Validate config
    config.http_socket();
    let instance_uri = parse_instance_url(&config.instance_url)
        .expect("invalid instance URL");
    if !is_correct_uri_scheme(&instance_uri) {
        let message = "instance_url may have incorrect URL scheme";
        warnings.push(message.to_owned());
    };
    if let Some(ref webfinger_hostname) = config.webfinger_hostname {
        if !is_same_apex_domain(instance_uri.hostname().as_str(), webfinger_hostname) {
            panic!("invalid webfinger_hostname");
        };
    };
    if config.authentication_methods.is_empty() {
        panic!("authentication_methods must not be empty");
    };
    if !config.federation.ssrf_protection_enabled {
        let message = "SSRF protection disabled";
        warnings.push(message.to_owned());
    };
    if !config.federation.fep_1b12_full_enabled {
        let message = "federation.fep_1b12_full_enabled parameter is deprecated";
        warnings.push(message.to_owned());
    };
    if config.blocked_instances.is_some() {
        let message = "blocked_instances parameter is deprecated (use `mitra add-filter-rule`)";
        warnings.push(message.to_owned());
    };
    if config.allowed_instances.is_some() {
        let message = "allowed_instances parameter is deprecated (use `mitra add-filter-rule`)";
        warnings.push(message.to_owned());
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
