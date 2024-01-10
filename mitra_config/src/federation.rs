use serde::Deserialize;

const fn default_federation_enabled() -> bool { true }
const fn default_fetcher_timeout() -> u64 { 300 }
const fn default_deliverer_timeout() -> u64 { 30 }
const fn default_deliverer_log_response_length() -> usize { 75 }
const fn default_fep_e232_enabled() -> bool { false }
const fn default_fep_8b32_eddsa_enabled() -> bool { false }

#[derive(Clone, Deserialize)]
pub struct FederationConfig {
    #[serde(default = "default_federation_enabled")]
    pub enabled: bool,

    #[serde(default = "default_fetcher_timeout")]
    pub(super) fetcher_timeout: u64,
    #[serde(default = "default_deliverer_timeout")]
    pub(super) deliverer_timeout: u64,
    #[serde(default = "default_deliverer_log_response_length")]
    pub(super) deliverer_log_response_length: usize,

    pub(super) proxy_url: Option<String>,
    pub(super) onion_proxy_url: Option<String>,
    pub(super) i2p_proxy_url: Option<String>,

    #[serde(default = "default_fep_e232_enabled")]
    pub fep_e232_enabled: bool,
    #[serde(default = "default_fep_8b32_eddsa_enabled")]
    pub fep_8b32_eddsa_enabled: bool,
}

impl Default for FederationConfig {
    fn default() -> Self {
        Self {
            enabled: default_federation_enabled(),
            fetcher_timeout: default_fetcher_timeout(),
            deliverer_timeout: default_deliverer_timeout(),
            deliverer_log_response_length: default_deliverer_log_response_length(),
            proxy_url: None,
            onion_proxy_url: None,
            i2p_proxy_url: None,
            fep_e232_enabled: default_fep_e232_enabled(),
            fep_8b32_eddsa_enabled: default_fep_8b32_eddsa_enabled(),
        }
    }
}
