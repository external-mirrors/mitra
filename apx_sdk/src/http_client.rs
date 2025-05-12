use std::cmp::max;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use bytes::{BufMut, Bytes, BytesMut};
use reqwest::{
    redirect::{Policy as RedirectPolicy},
    Client,
    Proxy,
    Response,
};
use thiserror::Error;

use apx_core::{
    http_url::parse_http_url_whatwg,
    url::hostname::{is_i2p, is_onion},
    urls::{
        get_hostname,
        get_ip_address,
        UrlError,
    },
};

use super::agent::FederationAgent;

const CONNECTION_TIMEOUT: u64 = 30;
pub const REDIRECT_LIMIT: usize = 3;

pub enum Network {
    Default,
    Tor,
    I2p,
}

pub fn get_network_type(request_url: &str) ->
    Result<Network, UrlError>
{
    let hostname = get_hostname(request_url)?;
    let network = if is_onion(&hostname) {
        Network::Tor
    } else if is_i2p(&hostname) {
        Network::I2p
    } else {
        Network::Default
    };
    Ok(network)
}

// https://www.w3.org/TR/activitypub/#security-localhost
// https://cheatsheetseries.owasp.org/cheatsheets/Server_Side_Request_Forgery_Prevention_Cheat_Sheet.html
fn is_safe_addr(ip_addr: IpAddr) -> bool {
    match ip_addr {
        IpAddr::V4(addr_v4) => !addr_v4.is_loopback() && !addr_v4.is_private(),
        IpAddr::V6(addr_v6) => !addr_v6.is_loopback(),
    }
}

/// Returns false if untrusted URL is not safe for fetching
fn is_safe_url(url: &str) -> bool {
    if let Ok(url) = parse_http_url_whatwg(url) {
        if let Some(ip_address) = get_ip_address(&url) {
            is_safe_addr(ip_address)
        } else {
            // Don't resolve domain name
            true
        }
    } else {
        // Not a valid 'http' URL
        false
    }
}

#[derive(Debug, Error)]
#[error("unsafe URL: {0}")]
pub struct UnsafeUrlError(String);

pub fn require_safe_url(url: &str) -> Result<(), UnsafeUrlError> {
    if !is_safe_url(url) {
        return Err(UnsafeUrlError(url.to_string()));
    };
    Ok(())
}

fn build_safe_redirect_policy() -> RedirectPolicy {
    RedirectPolicy::custom(|attempt| {
        if attempt.previous().len() > REDIRECT_LIMIT {
            attempt.error("too many redirects")
        } else if !is_safe_url(attempt.url().as_str()) {
            attempt.stop()
        } else {
            attempt.follow()
        }
    })
}

mod dns_resolver {
    // https://github.com/seanmonstar/reqwest/blob/v0.12.4/src/dns/gai.rs
    use futures_util::future::FutureExt;
    use hyper_util::client::legacy::connect::dns::GaiResolver as HyperGaiResolver;
    use reqwest::dns::{Addrs, Name, Resolve, Resolving};
    use tower_service::{Service as _};

    use super::is_safe_addr;

    type BoxError = Box<dyn std::error::Error + Send + Sync>;

    pub struct SafeResolver(HyperGaiResolver);

    impl SafeResolver {
        pub fn new() -> Self {
            Self(HyperGaiResolver::new())
        }
    }

    impl Resolve for SafeResolver {
        fn resolve(&self, name: Name) -> Resolving {
            let this = &mut self.0.clone();
            let hyper_name = name.as_str().parse()
                .expect("domain name should be valid");
            Box::pin(this.call(hyper_name).map(|result| {
                result
                    .map(|addrs| -> Addrs {
                        Box::new(addrs.filter(|addr| is_safe_addr(addr.ip())))
                    })
                    .map_err(|err| -> BoxError { Box::new(err) })
            }))
        }
    }
}

pub fn build_http_client(
    agent: &FederationAgent,
    network: Network,
    timeout: u64,
    no_redirect: bool,
) -> reqwest::Result<Client> {
    let mut client_builder = Client::builder();
    let mut maybe_proxy_url = agent.proxy_url.as_ref();
    match network {
        Network::Default => (),
        Network::Tor => {
            maybe_proxy_url = agent.onion_proxy_url.as_ref()
                .or(maybe_proxy_url);
        },
        Network::I2p => {
            maybe_proxy_url = agent.i2p_proxy_url.as_ref()
                .or(maybe_proxy_url);
        },
    };
    if let Some(proxy_url) = maybe_proxy_url {
        let proxy = Proxy::all(proxy_url)?;
        client_builder = client_builder.proxy(proxy);
    };
    if agent.ssrf_protection_enabled {
        client_builder = client_builder.dns_resolver(
            Arc::new(dns_resolver::SafeResolver::new()));
    };
    let redirect_policy = if no_redirect {
        RedirectPolicy::none()
    } else if agent.ssrf_protection_enabled {
        build_safe_redirect_policy()
    } else {
        RedirectPolicy::limited(REDIRECT_LIMIT)
    };
    let request_timeout = Duration::from_secs(timeout);
    let connect_timeout = Duration::from_secs(max(
        timeout,
        CONNECTION_TIMEOUT,
    ));
    client_builder
        .timeout(request_timeout)
        .connect_timeout(connect_timeout)
        .redirect(redirect_policy)
        .build()
}

// Workaround for https://github.com/seanmonstar/reqwest/issues/1234
pub async fn limited_response(
    response: &mut Response,
    limit: usize,
) -> Result<Option<Bytes>, reqwest::Error> {
    let mut bytes = BytesMut::new();
    while let Some(chunk) = response.chunk().await? {
        let len = bytes.len() + chunk.len();
        if len > limit {
            return Ok(None);
        }
        bytes.put(chunk);
    };
    Ok(Some(bytes.freeze()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_safe_url() {
        assert_eq!(is_safe_url("https://server.example/test"), true);
        assert_eq!(is_safe_url("http://bq373nez4.onion/test"), true);
        assert_eq!(is_safe_url("ftp://user@server.example"), false);
        assert_eq!(is_safe_url("file:///etc/passwd"), false);
        assert_eq!(is_safe_url("http://127.0.0.1:5941/test"), false);
        assert_eq!(is_safe_url("http://[::1]:5941/test"), false);
        assert_eq!(is_safe_url("http://localhost:5941/test"), true);
        assert_eq!(is_safe_url("https://server.local/test"), true);
    }
}
