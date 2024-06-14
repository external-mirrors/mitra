use serde::Deserialize;

const fn default_federation_enabled() -> bool { true }
const fn default_inbox_queue_batch_size() -> u32 { 20 }
const fn default_fetcher_timeout() -> u64 { 300 }
const fn default_deliverer_timeout() -> u64 { 30 }
const fn default_deliverer_log_response_length() -> usize { 75 }
const fn default_fep_e232_enabled() -> bool { false }
const fn default_announce_like_enabled() -> bool { true }

#[derive(Clone, Deserialize)]
pub struct FederationConfig {
    #[serde(default = "default_federation_enabled")]
    pub enabled: bool,

    #[serde(default = "default_inbox_queue_batch_size")]
    pub inbox_queue_batch_size: u32,

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
    #[serde(default = "default_announce_like_enabled")]
    pub announce_like_enabled: bool,
}

impl Default for FederationConfig {
    fn default() -> Self {
        Self {
            enabled: default_federation_enabled(),
            inbox_queue_batch_size: default_inbox_queue_batch_size(),
            fetcher_timeout: default_fetcher_timeout(),
            deliverer_timeout: default_deliverer_timeout(),
            deliverer_log_response_length: default_deliverer_log_response_length(),
            proxy_url: None,
            onion_proxy_url: None,
            i2p_proxy_url: None,
            fep_e232_enabled: default_fep_e232_enabled(),
            announce_like_enabled: default_announce_like_enabled(),
        }
    }
}
