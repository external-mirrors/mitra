use std::collections::HashMap;
use std::path::PathBuf;

use log::{Level as LogLevel};
use serde::Deserialize;

use mitra_utils::{
    crypto_eddsa::Ed25519SecretKey,
    crypto_rsa::RsaSecretKey,
    urls::{get_hostname, normalize_origin, UrlError},
};

use super::authentication::{
    default_authentication_methods,
    default_authentication_token_lifetime,
    default_login_message,
    AuthenticationMethod,
};
use super::blockchain::{
    BlockchainConfig,
    EthereumConfig,
    MoneroConfig,
};
use super::environment::Environment;
use super::federation::FederationConfig;
use super::limits::Limits;
use super::registration::RegistrationConfig;
use super::retention::RetentionConfig;
use super::{SOFTWARE_NAME, SOFTWARE_VERSION};

fn default_log_level() -> LogLevel { LogLevel::Info }

const fn default_instance_staff_public() -> bool { true }

#[derive(Clone, Deserialize)]
pub struct Config {
    // Properties auto-populated from the environment
    #[serde(skip)]
    pub environment: Environment,

    #[serde(skip)]
    pub config_path: String,

    // Core settings
    pub database_url: String,
    /// TLS certificate authority file path for validating the database secure connection
    pub database_tls_ca_file: Option<PathBuf>,

    pub storage_dir: PathBuf,
    pub web_client_dir: Option<PathBuf>,

    pub http_host: String,
    pub http_port: u32,

    #[serde(default)]
    pub http_cors_allowlist: Vec<String>,

    #[serde(default = "default_log_level")]
    pub log_level: LogLevel,

    // Domain name or <IP address>:<port>
    // URI scheme is optional
    instance_uri: String,

    pub instance_title: String,
    pub instance_short_description: String,
    pub instance_description: String,
    #[serde(default = "default_instance_staff_public")]
    pub instance_staff_public: bool,
    #[serde(default)]
    pub instance_timeline_public: bool,

    #[serde(skip)]
    instance_ed25519_key: Option<Ed25519SecretKey>,
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
    #[serde(rename = "blockchain")]
    _blockchain: Option<BlockchainConfig>, // deprecated
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

    pub(super) fn try_instance_url(&self) -> Result<String, UrlError> {
        normalize_origin(&self.instance_uri)
    }

    pub fn instance(&self) -> Instance {
        Instance {
            _url: self.try_instance_url()
                .expect("instance URL should be already validated"),
            actor_ed25519_key: self.instance_ed25519_key
                .expect("instance Ed25519 key should be already generated"),
            actor_rsa_key: self.instance_rsa_key.clone()
                .expect("instance RSA key should be already generated"),
            proxy_url: self.federation.proxy_url.clone(),
            onion_proxy_url: self.federation.onion_proxy_url.clone(),
            i2p_proxy_url: self.federation.i2p_proxy_url.clone(),
            // Private instance doesn't send activities and sign requests
            is_private:
                !self.federation.enabled ||
                matches!(self.environment, Environment::Development),
            fetcher_timeout: self.federation.fetcher_timeout,
            deliverer_timeout: self.federation.deliverer_timeout,
            deliverer_log_response_length: self.federation.deliverer_log_response_length,
            fep_8b32_eddsa_enabled: self.federation.fep_8b32_eddsa_enabled,
        }
    }

    pub fn instance_url(&self) -> String {
        self.instance().url()
    }

    pub fn blockchains(&self) -> &[BlockchainConfig] {
        if let Some(ref _blockchain_config) = self._blockchain {
            panic!("'blockchain' setting is not supported anymore, use 'blockchains' instead");
        } else {
            let is_error = self.blockchains.iter()
                .fold(HashMap::new(), |mut map, blockchain_config| {
                    let key = match blockchain_config {
                        BlockchainConfig::Ethereum(_) => 1,
                        BlockchainConfig::Monero(_) => 2,
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
    }

    pub fn ethereum_config(&self) -> Option<&EthereumConfig> {
        self.blockchains().iter()
            .find_map(|item| match item {
                BlockchainConfig::Ethereum(config) => Some(config),
                _ => None,
            })
    }

    pub fn monero_config(&self) -> Option<&MoneroConfig> {
        self.blockchains().iter()
            .find_map(|item| match item {
                BlockchainConfig::Monero(config) => Some(config),
                _ => None,
            })
    }
}

#[derive(Clone)]
pub struct Instance {
    _url: String,
    // Instance actor keys
    pub actor_ed25519_key: Ed25519SecretKey,
    pub actor_rsa_key: RsaSecretKey,
    // Proxy for outgoing requests
    pub proxy_url: Option<String>,
    pub onion_proxy_url: Option<String>,
    pub i2p_proxy_url: Option<String>,
    // Private instance won't send signed HTTP requests
    pub is_private: bool,
    pub fetcher_timeout: u64,
    pub deliverer_timeout: u64,
    pub deliverer_log_response_length: usize,

    pub fep_8b32_eddsa_enabled: bool,
}

impl Instance {
    pub fn url(&self) -> String {
        self._url.clone()
    }

    /// Returns instance host name (without port number)
    pub fn hostname(&self) -> String {
        get_hostname(&self._url)
            // URL is being validated at instantiation
            .expect("instance URL should have hostname")
    }

    pub fn agent(&self) -> String {
        format!(
            "{name} {version}; {instance_url}",
            name=SOFTWARE_NAME,
            version=SOFTWARE_VERSION,
            instance_url=self.url(),
        )
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl Instance {
    pub fn for_test(url: &str) -> Self {
        use mitra_utils::{
            crypto_eddsa::generate_weak_ed25519_key,
            crypto_rsa::generate_weak_rsa_key,
        };
        Self {
            _url: normalize_origin(url).unwrap(),
            actor_rsa_key: generate_weak_rsa_key().unwrap(),
            actor_ed25519_key: generate_weak_ed25519_key(),
            proxy_url: None,
            onion_proxy_url: None,
            i2p_proxy_url: None,
            is_private: true,
            fetcher_timeout: 0,
            deliverer_timeout: 0,
            deliverer_log_response_length: 0,
            fep_8b32_eddsa_enabled: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_instance_url_https_dns() {
        let instance_url = "https://example.com/";
        let instance = Instance::for_test(instance_url);

        assert_eq!(instance.url(), "https://example.com");
        assert_eq!(instance.hostname(), "example.com");
        assert_eq!(
            instance.agent(),
            format!("Mitra {}; https://example.com", SOFTWARE_VERSION),
        );
    }

    #[test]
    fn test_instance_url_http_ipv4_with_port() {
        let instance_url = "http://1.2.3.4:3777/";
        let instance = Instance::for_test(instance_url);

        assert_eq!(instance.url(), "http://1.2.3.4:3777");
        assert_eq!(instance.hostname(), "1.2.3.4");
    }
}
