use std::collections::HashMap;
use std::path::PathBuf;

use log::{Level as LogLevel};
use serde::Deserialize;

use apx_core::{
    crypto_eddsa::Ed25519SecretKey,
    crypto_rsa::RsaSecretKey,
};

use super::authentication::{
    default_authentication_methods,
    default_authentication_token_lifetime,
    default_login_message,
    AuthenticationMethod,
};
use super::blockchain::{
    BlockchainConfig,
    MoneroConfig,
};
use super::environment::Environment;
use super::federation::FederationConfig;
use super::instance::Instance;
use super::limits::Limits;
use super::registration::RegistrationConfig;
use super::retention::RetentionConfig;

fn default_log_level() -> LogLevel { LogLevel::Info }

const fn default_web_client_rewrite_index() -> bool { true }

const fn default_instance_staff_public() -> bool { true }

#[derive(Clone, Deserialize)]
pub struct Config {
    // Properties auto-populated from the environment
    #[serde(skip)]
    pub environment: Environment,

    #[serde(skip)]
    pub config_path: String,

    #[serde(default = "default_log_level")]
    pub log_level: LogLevel,

    // Core settings
    pub database_url: String,
    /// TLS certificate authority file path for validating the database secure connection
    pub database_tls_ca_file: Option<PathBuf>,

    pub storage_dir: PathBuf,

    pub web_client_dir: Option<PathBuf>,
    pub web_client_theme_dir: Option<PathBuf>,
    #[serde(default = "default_web_client_rewrite_index")]
    pub web_client_rewrite_index: bool,

    pub http_host: Option<String>,
    pub http_port: Option<u32>,
    // Overrides http_host and http_port
    pub http_socket: Option<String>,
    // Unix socket permissions (example: 0o640)
    pub http_socket_perms: Option<u32>,

    #[serde(default)]
    pub http_cors_allowlist: Vec<String>,

    // Domain name or <IP address>:<port>
    // URI scheme is optional
    pub(super) instance_uri: String,

    pub instance_title: String,
    pub instance_short_description: String,
    pub instance_description: String,
    #[serde(default = "default_instance_staff_public")]
    pub instance_staff_public: bool,
    #[serde(default)]
    pub instance_timeline_public: bool,

    #[serde(skip)]
    pub(super) instance_ed25519_key: Option<Ed25519SecretKey>,
    #[serde(skip)]
    pub(super) instance_rsa_key: Option<RsaSecretKey>,

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

    #[serde(default)]
    pub blocked_instances: Vec<String>,
    #[serde(default)]
    pub allowed_instances: Vec<String>,

    // Blockchain integrations
    #[serde(default)]
    blockchains: Vec<BlockchainConfig>,

    // IPFS
    pub ipfs_api_url: Option<String>,
    pub ipfs_gateway_url: Option<String>,
}

impl Config {
    pub fn set_instance_ed25519_key(&mut self, secret_key: Ed25519SecretKey) -> () {
        assert!(
            self.instance_ed25519_key.is_none(),
            "instance Ed25519 key can not be replaced",
        );
        self.instance_ed25519_key = Some(secret_key);
    }

    pub fn get_instance_rsa_key(&self) -> Option<&RsaSecretKey> {
        self.instance_rsa_key.as_ref()
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
            (Some(http_socket), _, _) => http_socket.to_string(),
            (None, Some(http_host), Some(http_port)) => {
                format!("{http_host}:{http_port}")
            },
            _ => panic!("either http_socket or http_host and http_port must be specified"),
        }
    }

    pub fn instance(&self) -> Instance {
        Instance::from_config(self)
    }

    pub fn instance_url(&self) -> String {
        self.instance().url()
    }

    pub fn blockchains(&self) -> &[BlockchainConfig] {
        let is_error = self.blockchains.iter()
            .fold(HashMap::new(), |mut map, blockchain_config| {
                let key = match blockchain_config {
                    BlockchainConfig::Monero(_) => 1,
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

    #[allow(clippy::unnecessary_find_map)]
    pub fn monero_config(&self) -> Option<&MoneroConfig> {
        self.blockchains().iter()
            .find_map(|item| match item {
                BlockchainConfig::Monero(config) => Some(config),
            })
    }
}
