use apx_core::crypto_rsa::RsaSecretKey;

pub struct FederationAgent {
    pub user_agent: String,
    // Private instance won't send signed HTTP requests
    pub is_instance_private: bool,
    // https://www.w3.org/TR/activitypub/#security-localhost
    pub ssrf_protection_enabled: bool,

    pub response_size_limit: usize,
    pub fetcher_timeout: u64,
    pub deliverer_timeout: u64,
    pub deliverer_log_response_length: usize,

    // Proxy for outgoing requests
    pub proxy_url: Option<String>,
    pub onion_proxy_url: Option<String>,
    pub i2p_proxy_url: Option<String>,

    pub signer_key: RsaSecretKey,
    pub signer_key_id: String,
}
