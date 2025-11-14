use std::collections::HashMap;
use std::net::Ipv6Addr;
use std::path::PathBuf;

use apx_core::{
    crypto::{
        eddsa::Ed25519SecretKey,
        rsa::RsaSecretKey,
    },
};
use log::{Level as LogLevel};
use serde::Deserialize;

use super::authentication::{
    default_authentication_methods,
    default_authentication_token_lifetime,
    default_login_message,
    AuthenticationMethod,
};
use super::blockchain::{
    BlockchainConfig,
    MoneroConfig,
    MoneroLightConfig,
};
use super::environment::Environment;
use super::federation::FederationConfig;
use super::instance::Instance;
use super::limits::Limits;
use super::metrics::Metrics;
use super::registration::RegistrationConfig;
use super::retention::RetentionConfig;

fn default_log_level() -> LogLevel { LogLevel::Info }

const fn default_web_client_rewrite_index() -> bool { true }
const fn default_media_proxy_enabled() -> bool { true }

const fn default_instance_staff_public() -> bool { true }

#[derive(Clone, Deserialize)]
pub struct Config {
    // Properties auto-populated from the environment
    #[serde(skip)]
    pub environment: Environment,

    #[serde(skip)]
    pub config_path: String,

    // Core settings
    #[serde(default = "default_log_level")]
    pub log_level: LogLevel,

    pub database_url: String,
    pub database_connection_pool_size: Option<usize>,
    /// TLS certificate authority file path for validating the database secure connection
    pub database_tls_ca_file: Option<PathBuf>,

    pub storage_dir: PathBuf,

    pub web_client_dir: Option<PathBuf>,
    pub web_client_theme_dir: Option<PathBuf>,
    #[serde(default = "default_web_client_rewrite_index")]
    pub web_client_rewrite_index: bool,
    #[serde(default = "default_media_proxy_enabled")]
    pub media_proxy_enabled: bool,

    pub http_host: Option<String>,
    pub http_port: Option<u32>,
    // Overrides http_host and http_port
    pub http_socket: Option<String>,
    // Unix socket permissions (example: 0o640)
    pub http_socket_perms: Option<u32>,

    pub http_cors_allowlist: Option<Vec<String>>,
    #[serde(default)]
    pub http_cors_allow_all: bool,

    // Domain name or <IP address>:<port>
    // URI scheme is optional
    #[serde(alias = "instance_uri")]
    pub(super) instance_url: String,

    pub instance_title: String,
    pub instance_short_description: String,
    pub instance_description: String,
    #[serde(default = "default_instance_staff_public")]
    pub instance_staff_public: bool,
    #[serde(default)]
    pub instance_timeline_public: bool,

    #[serde(default)]
    pub registration: RegistrationConfig,

    #[serde(default = "default_authentication_methods")]
    pub authentication_methods: Vec<AuthenticationMethod>,

    #[serde(default = "default_authentication_token_lifetime")]
    pub authentication_token_lifetime: u32,

    // EIP-4361 login message
    #[serde(default = "default_login_message")]
    pub login_message: String,

    #[serde(default)]
    pub limits: Limits,

    #[serde(default)]
    pub retention: RetentionConfig,

    #[serde(default)]
    pub federation: FederationConfig,

    pub blocked_instances: Option<Vec<String>>,
    pub allowed_instances: Option<Vec<String>>,

    pub metrics: Option<Metrics>,

    // Blockchain integrations
    #[serde(default)]
    blockchains: Vec<BlockchainConfig>,

    // IPFS
    pub ipfs_api_url: Option<String>,
    pub ipfs_gateway_url: Option<String>,

    // Fields that are populated during init phase
    #[serde(skip)]
    pub(super) instance_ed25519_key: Option<Ed25519SecretKey>,
    #[serde(skip)]
    pub(super) instance_rsa_key: Option<RsaSecretKey>,
}

impl Config {
    pub fn set_instance_ed25519_key(&mut self, secret_key: Ed25519SecretKey) -> () {
        assert!(
            self.instance_ed25519_key.is_none(),
            "instance Ed25519 key can not be replaced",
        );
        self.instance_ed25519_key = Some(secret_key);
    }

    pub fn set_instance_rsa_key(&mut self, secret_key: RsaSecretKey) -> () {
        assert!(
            self.instance_rsa_key.is_none(),
            "instance RSA key can not be replaced",
        );
        self.instance_rsa_key = Some(secret_key);
    }

    pub fn http_socket(&self) -> String {
        match (&self.http_socket, &self.http_host, self.http_port) {
            (Some(http_socket), _, _) => http_socket.clone(),
            (None, Some(http_host), Some(http_port)) => {
                if http_host.parse::<Ipv6Addr>().is_ok() {
                    format!("[{http_host}]:{http_port}")
                } else {
                    format!("{http_host}:{http_port}")
                }
            },
            _ => panic!("either http_socket or http_host and http_port must be specified"),
        }
    }

    pub fn instance(&self) -> Instance {
        Instance::from_config(self)
    }

    pub fn blockchains(&self) -> &[BlockchainConfig] {
        let is_error = self.blockchains.iter()
            .fold(HashMap::new(), |mut map, blockchain_config| {
                let key = match blockchain_config {
                    BlockchainConfig::Monero(_) => 1,
                    BlockchainConfig::MoneroLight(_) => 2,
                };
                map.entry(key)
                    .and_modify(|count| *count += 1)
                    .or_insert(1);
                map
            })
            .into_values()
            .any(|count| count > 1);
        if is_error {
            panic!("'blockchains' array contains more than one chain of the same kind");
        };
        &self.blockchains
    }

    pub fn monero_config(&self) -> Option<&MoneroConfig> {
        self.blockchains().iter()
            .find_map(|item| match item {
                BlockchainConfig::Monero(config) => Some(config),
                _ => None,
            })
    }

    pub fn monero_light_config(&self) -> Option<&MoneroLightConfig> {
        self.blockchains().iter()
            .find_map(|item| match item {
                BlockchainConfig::MoneroLight(config) => Some(config),
                _ => None,
            })
    }
}
