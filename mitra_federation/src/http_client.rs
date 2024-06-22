use std::cmp::max;
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

use mitra_utils::urls::{get_hostname, is_safe_url, UrlError};

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
    let network = if hostname.ends_with(".onion") {
        Network::Tor
    } else if hostname.ends_with(".i2p") {
        Network::I2p
    } else {
        Network::Default
    };
    Ok(network)
}

#[derive(Debug, Error)]
#[error("unsafe URL")]
pub struct UnsafeUrlError;

pub fn require_safe_url(url: &str) -> Result<(), UnsafeUrlError> {
    if !is_safe_url(url) {
        return Err(UnsafeUrlError);
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
    // https://github.com/seanmonstar/reqwest/blob/v0.11.27/src/dns/gai.rs
    use std::net::{IpAddr, SocketAddr};

    use futures_util::future::FutureExt;
    use hyper::client::connect::dns::{GaiResolver as HyperGaiResolver, Name};
    use hyper::service::Service;
    use reqwest::dns::{Addrs, Resolve, Resolving};

    type BoxError = Box<dyn std::error::Error + Send + Sync>;

    pub struct SafeResolver(HyperGaiResolver);

    impl SafeResolver {
        pub fn new() -> Self {
            Self(HyperGaiResolver::new())
        }
    }

    // https://www.w3.org/TR/activitypub/#security-localhost
    fn is_safe_addr(addr: &SocketAddr) -> bool {
        match addr.ip() {
            IpAddr::V4(addr_v4) => !addr_v4.is_loopback(),
            IpAddr::V6(addr_v6) => !addr_v6.is_loopback(),
        }
    }

    impl Resolve for SafeResolver {
        fn resolve(&self, name: Name) -> Resolving {
            let this = &mut self.0.clone();
            Box::pin(Service::<Name>::call(this, name).map(|result| {
                result
                    .map(|addrs| -> Addrs {
                        Box::new(addrs.filter(is_safe_addr))
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
    if cfg!(feature = "rustls-tls") {
        // https://github.com/hyperium/hyper/issues/3427
        client_builder = client_builder.http1_only();
    };
    let redirect_policy = if no_redirect {
        RedirectPolicy::none()
    } else {
        build_safe_redirect_policy()
    };
    let request_timeout = Duration::from_secs(timeout);
    let connect_timeout = Duration::from_secs(max(
        timeout,
        CONNECTION_TIMEOUT,
    ));
    client_builder
        .dns_resolver(Arc::new(dns_resolver::SafeResolver::new()))
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
