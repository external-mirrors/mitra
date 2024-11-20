use apx_core::crypto_rsa::RsaSecretKey;

pub struct RequestSigner {
    pub key: RsaSecretKey,
    pub key_id: String,
}

pub struct FederationAgent {
    /// User-Agent string.
    pub user_agent: Option<String>,
    // https://www.w3.org/TR/activitypub/#security-localhost
    pub ssrf_protection_enabled: bool,

    pub response_size_limit: usize,
    pub fetcher_timeout: u64,
    pub deliverer_timeout: u64,

    // Proxy for outgoing requests
    pub proxy_url: Option<String>,
    pub onion_proxy_url: Option<String>,
    pub i2p_proxy_url: Option<String>,

    /// Key for creating HTTP signatures.
    pub signer: Option<RequestSigner>,
}
