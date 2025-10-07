use std::cmp::max;
use std::error::{Error as _};
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use http_body_util::{BodyExt, Limited};
use reqwest::{
    header,
    redirect::{Policy as RedirectPolicy},
    Body,
    Client,
    Error,
    Method,
    Proxy,
    RequestBuilder,
    Response,
};
use thiserror::Error;

use apx_core::{
    http_signatures::create::{
        create_http_signature_cavage,
        create_http_signature_rfc9421,
        HttpSignatureError,
        HttpSigner,
    },
    http_url::parse_http_url_whatwg,
    http_url_whatwg::{
        get_hostname,
        get_ip_address,
        UrlError,
    },
    url::hostname::{is_i2p, is_onion},
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

pub enum RedirectAction {
    None,
    Follow,
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

fn require_safe_url(url: &str) -> Result<(), UnsafeUrlError> {
    if !is_safe_url(url) {
        return Err(UnsafeUrlError(url.to_string()));
    };
    Ok(())
}

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

pub fn create_http_client(
    agent: &FederationAgent,
    network: Network,
    timeout: u64,
    redirect_action: RedirectAction,
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
