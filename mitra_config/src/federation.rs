use serde::Deserialize;

const fn default_federation_enabled() -> bool { true }
const fn default_ssrf_protection_enabled() -> bool { true }
const fn default_incoming_queue_worker_enabled() -> bool { true }
const fn default_inbox_queue_batch_size() -> u32 { 20 }
const fn default_fetcher_timeout() -> u64 { 60 }
const fn default_deliverer_standalone() -> bool { true }
const fn default_deliverer_pool_size() -> usize { 10 }
const fn default_deliverer_timeout() -> u64 { 30 }
const fn default_deliverer_log_response_length() -> usize { 75 }
const fn default_fep_e232_enabled() -> bool { true }
const fn default_fep_1b12_full_enabled() -> bool { true }
const fn default_fep_c0e0_emoji_react_enabled() -> bool { false }

#[derive(Clone, Deserialize)]
pub struct FederationConfig {
    #[serde(default = "default_federation_enabled")]
    pub enabled: bool,

    #[serde(default = "default_ssrf_protection_enabled")]
    pub ssrf_protection_enabled: bool,

    // TODO: rename to incoming_queue_batch_size
    #[serde(default = "default_inbox_queue_batch_size")]
    pub inbox_queue_batch_size: u32,

    #[serde(default = "default_incoming_queue_worker_enabled")]
    pub incoming_queue_worker_enabled: bool,

    #[serde(default = "default_fetcher_timeout")]
    pub fetcher_timeout: u64,
    #[serde(default = "default_deliverer_timeout")]
    pub deliverer_timeout: u64,
    #[serde(default = "default_deliverer_log_response_length")]
    pub deliverer_log_response_length: usize,
    #[serde(default = "default_deliverer_standalone")]
    pub deliverer_standalone: bool,
    #[serde(default = "default_deliverer_pool_size")]
    pub deliverer_pool_size: usize,

    // Proxy for outgoing requests
    pub proxy_url: Option<String>,
    pub onion_proxy_url: Option<String>,
    pub i2p_proxy_url: Option<String>,

    #[serde(default = "default_fep_e232_enabled")]
    pub fep_e232_enabled: bool,
    #[serde(
        alias = "announce_like_enabled",
        default = "default_fep_1b12_full_enabled",
    )]
    pub fep_1b12_full_enabled: bool,

    #[serde(default = "default_fep_c0e0_emoji_react_enabled")]
    pub fep_c0e0_emoji_react_enabled: bool,
}

impl Default for FederationConfig {
    fn default() -> Self {
        Self {
            enabled: default_federation_enabled(),
            ssrf_protection_enabled: default_ssrf_protection_enabled(),
            inbox_queue_batch_size: default_inbox_queue_batch_size(),
            incoming_queue_worker_enabled: default_incoming_queue_worker_enabled(),
            fetcher_timeout: default_fetcher_timeout(),
            deliverer_timeout: default_deliverer_timeout(),
            deliverer_log_response_length: default_deliverer_log_response_length(),
            deliverer_standalone: default_deliverer_standalone(),
            deliverer_pool_size: default_deliverer_pool_size(),
            proxy_url: None,
            onion_proxy_url: None,
            i2p_proxy_url: None,
            fep_e232_enabled: default_fep_e232_enabled(),
            fep_1b12_full_enabled: default_fep_1b12_full_enabled(),
            fep_c0e0_emoji_react_enabled: default_fep_c0e0_emoji_react_enabled(),
        }
    }
}
