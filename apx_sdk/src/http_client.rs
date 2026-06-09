use std::cmp::max;
use std::error::{Error as _};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::time::Duration;

use bytes::Bytes;
#[cfg(not(target_arch = "wasm32"))]
use http_body_util::{BodyExt, Limited};
use reqwest::{
    header,
    Client,
    Error,
    Method,
    RequestBuilder,
    Response,
};
#[cfg(not(target_arch = "wasm32"))]
use reqwest::{
    redirect::{Policy as RedirectPolicy},
    Body,
    Proxy,
};
use thiserror::Error;

use apx_core::{
    http_signatures::create::{
        create_http_signature_cavage,
        create_http_signature_rfc9421,
        HttpSignatureError,
        HttpSigner,
    },
    url::{
        hostname::{is_i2p, is_onion},
        http_uri::parse_http_url_whatwg,
        http_url_whatwg::{get_ip_address, Host},
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

fn get_network_type(hostname: &str) -> Network {
    if is_onion(hostname) {
        Network::Tor
    } else if is_i2p(hostname) {
        Network::I2p
    } else {
        Network::Default
    }
}

pub enum RedirectAction {
    None,
    Follow,
}

// https://www.w3.org/TR/activitypub/#security-localhost
// https://cheatsheetseries.owasp.org/cheatsheets/Server_Side_Request_Forgery_Prevention_Cheat_Sheet.html
fn is_safe_addr(ip_addr: IpAddr) -> bool {
    // Reference:
    // https://www.iana.org/assignments/iana-ipv4-special-registry/iana-ipv4-special-registry.xhtml
    let is_unsafe_ipv4 = |addr: Ipv4Addr| {
        addr.is_loopback()
        || addr.is_unspecified()
        || addr.is_private()
        || addr.is_link_local()
        // is_shared (Rust Unstable)
        || (addr.octets()[0] == 100 && (addr.octets()[1] & 0b1100_0000 == 0b0100_0000))
        // is_benchmarking (Rust Unstable)
        || (addr.octets()[0] == 198 && (addr.octets()[1] & 0xfe) == 18)
        // Private addresses from 192.0.0.0/24 block
        || (addr.octets()[0] == 192
            && addr.octets()[1] == 0
            && addr.octets()[2] == 0
            && addr.octets()[3] != 9
            && addr.octets()[3] != 10)
    };
    // Reference:
    // https://www.iana.org/assignments/iana-ipv6-special-registry/iana-ipv6-special-registry.xhtml
    let is_unsafe_ipv6 = |addr: Ipv6Addr| {
        addr.is_loopback()
        || addr.is_unspecified()
        // is_unicast_link_local (Rust 1.84)
        || (addr.segments()[0] & 0xffc0) == 0xfe80
        // is_unique_local (Rust 1.84)
        || (addr.segments()[0] & 0xfe00) == 0xfc00
        // is_benchmarking (Rust Unstable)
        || ((addr.segments()[0] == 0x2001) && (addr.segments()[1] == 0x2) && (addr.segments()[2] == 0))
    };
    match ip_addr {
        IpAddr::V4(addr_v4) => !is_unsafe_ipv4(addr_v4),
        IpAddr::V6(addr_v6) => {
            let is_unsafe_mapped = addr_v6.to_ipv4_mapped()
                .is_some_and(is_unsafe_ipv4);
            !is_unsafe_ipv6(addr_v6) && !is_unsafe_mapped
        },
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

fn require_safe_url(url: &str) -> Result<(), UnsafeUrlError> {
    if !is_safe_url(url) {
        return Err(UnsafeUrlError(url.to_string()));
    };
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn create_safe_redirect_policy() -> RedirectPolicy {
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

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
pub fn create_http_client(
    agent: &FederationAgent,
    target_host: &Host<String>,
    timeout: u64,
    redirect_action: RedirectAction,
) -> reqwest::Result<Client> {
    let mut client_builder = Client::builder();
    let mut maybe_proxy_url = agent.proxy_url.as_ref();
    let network = get_network_type(&target_host.to_string());
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
        if !agent.no_proxy.iter()
            .any(|host| *host == target_host.to_string())
        {
            client_builder = client_builder.proxy(proxy);
        };
    };
    if agent.ssrf_protection_enabled {
        client_builder = client_builder.dns_resolver(
            dns_resolver::SafeResolver::new().into());
    };
    let redirect_policy = match redirect_action {
        RedirectAction::None => RedirectPolicy::none(),
        RedirectAction::Follow => {
            if agent.ssrf_protection_enabled {
                create_safe_redirect_policy()
            } else {
                RedirectPolicy::limited(REDIRECT_LIMIT)
            }
        },
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

#[cfg(target_arch = "wasm32")]
pub fn create_http_client(
    _agent: &FederationAgent,
    target_host: &Host<String>,
    timeout: u64,
    _redirect_action: RedirectAction,
) -> reqwest::Result<Client> {
    // Proxies are not supported:
    // https://github.com/seanmonstar/reqwest/issues/2504
    let _network = get_network_type(&target_host.to_string());

    // DNS resolvers are not supported:

    // Redirection policies are not supported:
    // https://github.com/seanmonstar/reqwest/issues/2071

    // Timeouts are not supported: https://github.com/seanmonstar/reqwest/pull/2850
    let _request_timeout = Duration::from_secs(timeout);
    let _connect_timeout = Duration::from_secs(max(
        timeout,
        CONNECTION_TIMEOUT,
    ));
    Client::builder().build()
}

pub fn build_http_request(
    agent: &FederationAgent,
    client: &Client,
    method: Method,
    target_url: &str,
) -> Result<RequestBuilder, UnsafeUrlError> {
    if agent.ssrf_protection_enabled {
        require_safe_url(target_url)?;
    };
    let mut request_builder = client.request(method, target_url);
    if let Some(ref user_agent) = agent.user_agent {
        request_builder = request_builder
            .header(header::USER_AGENT, user_agent);
    };
    Ok(request_builder)
}

pub fn sign_http_request(
    mut request_builder: RequestBuilder,
    method: Method,
    target_url: &str,
    maybe_body: Option<&[u8]>,
    signer: &HttpSigner,
    rfc9421_enabled: bool,
) -> Result<RequestBuilder, HttpSignatureError> {
    if rfc9421_enabled {
        let headers = create_http_signature_rfc9421(
            method,
            target_url,
            maybe_body,
            signer,
        )?;
        if let Some(content_digest) = headers.content_digest {
            request_builder = request_builder
                .header("Content-Digest", content_digest);
        };
        request_builder = request_builder
            .header("Signature", headers.signature)
            .header("Signature-Input", headers.signature_input);
    } else {
        let headers = create_http_signature_cavage(
            method,
            target_url,
            maybe_body,
            signer,
        )?;
        if let Some(digest) = headers.digest {
            request_builder = request_builder.header("Digest", digest);
        };
        request_builder = request_builder
            .header(header::HOST, headers.host)
            .header(header::DATE, headers.date)
            .header("Signature", headers.signature);
    };
    Ok(request_builder)
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn limited_response(
    response: Response,
    limit: usize,
) -> Option<Bytes> {
    Limited::new(Body::from(response), limit)
        .collect()
        .await
        .ok()
        .map(|collected| collected.to_bytes())
}

#[cfg(target_arch = "wasm32")]
pub async fn limited_response(
    response: Response,
    _limit: usize,
) -> Option<Bytes> {
    // Body::from(response) is not implemented:
    // https://github.com/seanmonstar/reqwest/pull/2837
    response.bytes().await.ok()
}

pub fn describe_request_error(error: &Error) -> String {
    if let Some(source) = error.source() {
        format!("{}: {}", error, source)
    } else {
        error.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_safe_addr_ipv4_private_networks() {
        let addresses = [
            "0.0.0.0",
            "10.0.0.1",
            "100.64.0.1",
            "127.0.0.1",
            "169.254.0.1",
            "172.16.0.1",
            "192.0.0.1",
            "192.168.0.1",
            "198.18.0.1",
        ];
        for address in addresses {
            let address = address.parse::<IpAddr>().unwrap();
            assert_eq!(is_safe_addr(address), false);
        };
    }

    #[test]
    fn test_is_safe_addr_ipv6_private_networks() {
        let addresses = [
            "::1",
            "::",
            "::ffff:0.0.0.0",
            "2001:2::",
            "fc00::",
            "fe80::",
        ];
        for address in addresses {
            let address = address.parse::<IpAddr>().unwrap();
            assert_eq!(is_safe_addr(address), false);
        };
    }

    #[test]
    fn test_is_safe_url() {
        assert_eq!(is_safe_url("https://server.example/test"), true);
        assert_eq!(is_safe_url("http://bq373nez4.onion/test"), true);
        assert_eq!(is_safe_url("ftp://user@server.example"), false);
        assert_eq!(is_safe_url("file:///etc/passwd"), false);
        assert_eq!(is_safe_url("http://127.0.0.1:5941/test"), false);
        assert_eq!(is_safe_url("http://[::1]:5941/test"), false);
        // These are supposed to be checked by the custom DNS resolver
        assert_eq!(is_safe_url("http://localhost:5941/test"), true);
        assert_eq!(is_safe_url("https://server.local/test"), true);
    }

    #[test]
    fn test_is_safe_url_yggdrasil() {
        let url = "http://[319:3cf0:dd1d:47b9:20c:29ff:fe2c:39be]/objects/1";
        assert_eq!(is_safe_url(url), true);
    }

    #[test]
    fn test_is_safe_url_unspecified() {
        let url = "http://0.0.0.0:8080/admin/";
        assert_eq!(is_safe_url(url), false);
    }

    #[test]
    fn test_is_safe_url_private() {
        let url = "http://172.17.0.1:8080/admin/";
        assert_eq!(is_safe_url(url), false);
        let url = "http://169.254.169.254/latest/meta-data/";
        assert_eq!(is_safe_url(url), false);
    }

    #[test]
    fn test_is_safe_url_ipv4_to_ipv6() {
        // 127.0.0.1 converted into IPv6 address
        let url = "http://[::ffff:7f00:1]:5941/test";
        assert_eq!(is_safe_url(url), false);
    }
}
