use apx_core::{
    crypto_eddsa::Ed25519SecretKey,
    crypto_rsa::RsaSecretKey,
    http_url::HttpUrl,
    urls::normalize_origin,
};

use super::{
    config::Config,
    environment::Environment,
    SOFTWARE_NAME,
    SOFTWARE_VERSION,
};

pub fn parse_instance_url(url: &str) -> Result<HttpUrl, &'static str> {
    let origin = normalize_origin(url).map_err(|_| "invalid URL")?;
    let http_url = HttpUrl::parse(&origin)?;
    Ok(http_url)
}

#[derive(Clone)]
pub struct Instance {
    _url: HttpUrl,
    // Instance actor keys
    pub actor_ed25519_key: Ed25519SecretKey,
    pub actor_rsa_key: RsaSecretKey,
    // Proxy for outgoing requests
    pub proxy_url: Option<String>,
    pub onion_proxy_url: Option<String>,
    pub i2p_proxy_url: Option<String>,
    // Private instance won't send signed HTTP requests
    pub is_private: bool,
    pub ssrf_protection_enabled: bool,
    pub fetcher_timeout: u64,
    pub deliverer_timeout: u64,
    pub deliverer_log_response_length: usize,
    pub deliverer_pool_size: usize,
}

impl Instance {
    pub(crate) fn from_config(config: &Config) -> Self {
        Self {
            _url: parse_instance_url(&config.instance_uri)
                .expect("instance URL should be already validated"),
            actor_ed25519_key: config.instance_ed25519_key
                .expect("instance Ed25519 key should be already generated"),
            actor_rsa_key: config.instance_rsa_key.clone()
                .expect("instance RSA key should be already generated"),
            proxy_url: config.federation.proxy_url.clone(),
            onion_proxy_url: config.federation.onion_proxy_url.clone(),
            i2p_proxy_url: config.federation.i2p_proxy_url.clone(),
            // Private instance doesn't send activities and sign requests
            is_private:
                !config.federation.enabled ||
                matches!(config.environment, Environment::Development),
            ssrf_protection_enabled: config.federation.ssrf_protection_enabled,
            fetcher_timeout: config.federation.fetcher_timeout,
            deliverer_timeout: config.federation.deliverer_timeout,
            deliverer_log_response_length: config.federation.deliverer_log_response_length,
            deliverer_pool_size: config.federation.deliverer_pool_size,
        }
    }

    pub fn url(&self) -> String {
        self._url.to_string()
    }

    /// Returns instance host name (without port number)
    pub fn hostname(&self) -> String {
        self._url.hostname().to_string()
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
        use apx_core::{
            crypto_eddsa::generate_weak_ed25519_key,
            crypto_rsa::generate_weak_rsa_key,
        };
        Self {
            _url: parse_instance_url(url).unwrap(),
            actor_rsa_key: generate_weak_rsa_key().unwrap(),
            actor_ed25519_key: generate_weak_ed25519_key(),
            proxy_url: None,
            onion_proxy_url: None,
            i2p_proxy_url: None,
            is_private: true,
            ssrf_protection_enabled: true,
            fetcher_timeout: 0,
            deliverer_timeout: 0,
            deliverer_log_response_length: 0,
            deliverer_pool_size: 0,
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
        // Test instance is private
        assert!(instance.is_private);
    }

    #[test]
    fn test_instance_url_http_ipv4_with_port() {
        let instance_url = "http://1.2.3.4:3777/";
        let instance = Instance::for_test(instance_url);

        assert_eq!(instance.url(), "http://1.2.3.4:3777");
        assert_eq!(instance.hostname(), "1.2.3.4");
    }
}
