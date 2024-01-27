use std::cmp::max;
use std::time::Duration;

use bytes::{BufMut, Bytes, BytesMut};
use reqwest::{
    redirect::{Policy as RedirectPolicy},
    Client,
    Proxy,
    Response,
};

use mitra_utils::urls::{get_hostname, UrlError};

use super::agent::FederationAgent;

const CONNECTION_TIMEOUT: u64 = 30;
const REDIRECT_LIMIT: usize = 3;

// See also: mitra_validators::posts::CONTENT_MAX_SIZE
pub const RESPONSE_SIZE_LIMIT: usize = 1_000_000;

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

pub fn build_http_client(
    agent: &FederationAgent,
    network: Network,
    timeout: u64,
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
    let redirect_policy = RedirectPolicy::limited(REDIRECT_LIMIT);
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
