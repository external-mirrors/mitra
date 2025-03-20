use apx_core::{
    crypto_eddsa::Ed25519SecretKey,
    crypto_rsa::RsaSecretKey,
    http_url::HttpUrl,
    urls::normalize_origin,
};

use super::{
    config::Config,
    environment::Environment,
    federation::FederationConfig,
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
    pub federation: FederationConfig,
    // Instance actor keys
    pub actor_ed25519_key: Ed25519SecretKey,
    pub actor_rsa_key: RsaSecretKey,
}

impl Instance {
    pub(crate) fn from_config(config: &Config) -> Self {
        let mut federation_config = config.federation.clone();
        if matches!(config.environment, Environment::Development) {
            // Private instance doesn't send activities and sign requests
            federation_config.enabled = false;
        };
        Self {
            _url: parse_instance_url(&config.instance_uri)
                .expect("instance URL should be already validated"),
            federation: federation_config,
            actor_ed25519_key: config.instance_ed25519_key
                .expect("instance Ed25519 key should be already generated"),
            actor_rsa_key: config.instance_rsa_key.clone()
                .expect("instance RSA key should be already generated"),
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
            federation: FederationConfig {
                enabled: false,
                ..Default::default()
            },
            actor_rsa_key: generate_weak_rsa_key().unwrap(),
            actor_ed25519_key: generate_weak_ed25519_key(),
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
        assert!(!instance.federation.enabled);
    }

    #[test]
    fn test_instance_url_http_ipv4_with_port() {
        let instance_url = "http://1.2.3.4:3777/";
        let instance = Instance::for_test(instance_url);

        assert_eq!(instance.url(), "http://1.2.3.4:3777");
        assert_eq!(instance.hostname(), "1.2.3.4");
    }
}
