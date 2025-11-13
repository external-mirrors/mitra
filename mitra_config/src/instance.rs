use apx_core::{
    crypto::{
        eddsa::Ed25519SecretKey,
        rsa::RsaSecretKey,
    },
    url::{
        hostname::{guess_protocol, is_ipv6_hostname},
        http_uri::{parse_http_url_whatwg, HttpUri},
    },
};

use super::{
    config::Config,
    environment::Environment,
    federation::FederationConfig,
    SOFTWARE_NAME,
    SOFTWARE_VERSION,
};

// Normalize HTTP origin:
// - add a scheme if it's missing
// - convert IDN to punycode
fn normalize_origin(url: &str) -> Result<String, &'static str> {
    let normalized_url = if
        url.starts_with("http://") ||
        url.starts_with("https://")
    {
        url.to_string()
    } else {
        // Add scheme
        let hostname = if is_ipv6_hostname(url) {
            url
        } else if let Some((hostname, _port)) = url.rsplit_once(':') {
            hostname
        } else {
            url
        };
        let url_scheme = guess_protocol(hostname);
        format!(
            "{}://{}",
            url_scheme,
            url,
        )
    };
    let url = parse_http_url_whatwg(&normalized_url)?;
    let origin = url.origin().ascii_serialization();
    Ok(origin)
}

pub fn parse_instance_url(url: &str) -> Result<HttpUri, &'static str> {
    let origin = normalize_origin(url)?;
    let http_uri = HttpUri::parse(&origin)?;
    Ok(http_uri)
}

pub fn is_correct_uri_scheme(uri: &HttpUri) -> bool {
    uri.scheme() == guess_protocol(uri.hostname().as_str())
}

#[derive(Clone)]
pub struct Instance {
    _uri: HttpUri,
    pub federation: FederationConfig,
    pub ed25519_secret_key: Ed25519SecretKey,
    pub rsa_secret_key: RsaSecretKey,
}

impl Instance {
    pub(crate) fn from_config(config: &Config) -> Self {
        let mut federation_config = config.federation.clone();
        if matches!(config.environment, Environment::Development) {
            // Private instance doesn't send activities and sign requests
            federation_config.enabled = false;
        };
        Self {
            _uri: parse_instance_url(&config.instance_url)
                .expect("instance URL should be already validated"),
            federation: federation_config,
            ed25519_secret_key: config.instance_ed25519_key
                .expect("instance Ed25519 key should be already generated"),
            rsa_secret_key: config.instance_rsa_key.clone()
                .expect("instance RSA key should be already generated"),
        }
    }

    pub fn uri(&self) -> &HttpUri {
        &self._uri
    }

    pub fn uri_str(&self) -> &str {
        self._uri.as_str()
    }

    /// Returns instance host name (without port number)
    pub fn hostname(&self) -> String {
        self._uri.hostname().to_string()
    }

    pub fn agent(&self) -> String {
        format!(
            "{name} {version}; {instance_uri}",
            name=SOFTWARE_NAME,
            version=SOFTWARE_VERSION,
            instance_uri=self.uri(),
        )
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl Instance {
    pub fn for_test(url: &str) -> Self {
        use apx_core::{
            crypto::{
                eddsa::generate_weak_ed25519_key,
                rsa::generate_weak_rsa_key,
            },
        };
        Self {
            _uri: parse_instance_url(url).unwrap(),
            federation: FederationConfig {
                enabled: false,
                ..Default::default()
            },
            rsa_secret_key: generate_weak_rsa_key().unwrap(),
            ed25519_secret_key: generate_weak_ed25519_key(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_origin() {
        let output = normalize_origin("https://social.example").unwrap();
        assert_eq!(output, "https://social.example");
        let output = normalize_origin("social.example").unwrap();
        assert_eq!(output, "https://social.example");
        // IDN
        let output = normalize_origin("嘟文.com").unwrap();
        assert_eq!(output, "https://xn--j5r817a.com");
        // IPv4 address
        let output = normalize_origin("127.0.0.1:8380").unwrap();
        assert_eq!(output, "http://127.0.0.1:8380");
        // Yggdrasil (IPv6) address
        let output = normalize_origin("[319:3cf0:dd1d:47b9:20c:29ff:fe2c:39be]").unwrap();
        assert_eq!(output, "http://[319:3cf0:dd1d:47b9:20c:29ff:fe2c:39be]");
        // Onion
        let output = normalize_origin("xyz.onion").unwrap();
        assert_eq!(output, "http://xyz.onion");
        // I2P
        let output = normalize_origin("http://xyz.i2p").unwrap();
        assert_eq!(output, "http://xyz.i2p");
        // I2P (no scheme)
        let output = normalize_origin("xyz.i2p").unwrap();
        assert_eq!(output, "http://xyz.i2p");
    }

    #[test]
    fn test_is_correct_uri_scheme() {
        let uri = HttpUri::parse("http://social.example").unwrap();
        let is_correct = is_correct_uri_scheme(&uri);
        assert_eq!(is_correct, false);
    }

    #[test]
    fn test_instance_url_https_dns() {
        let instance_url = "https://example.com/";
        let instance = Instance::for_test(instance_url);

        assert_eq!(instance.uri_str(), "https://example.com");
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

        assert_eq!(instance.uri_str(), "http://1.2.3.4:3777");
        assert_eq!(instance.hostname(), "1.2.3.4");
    }
}
