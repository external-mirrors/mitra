use std::cmp::max;
use std::time::Duration;

use bytes::{BufMut, Bytes, BytesMut};
use reqwest::{Client, Proxy, Response};

use mitra_config::Instance;
use mitra_utils::urls::get_hostname;

const CONNECTION_TIMEOUT: u64 = 30;

// See also: mitra_validators::posts::CONTENT_MAX_SIZE
pub const RESPONSE_SIZE_LIMIT: usize = 1_000_000;

pub enum Network {
    Default,
    Tor,
    I2p,
}

pub fn get_network_type(request_url: &str) ->
    Result<Network, url::ParseError>
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
    instance: &Instance,
    network: Network,
    timeout: u64,
) -> reqwest::Result<Client> {
    let mut client_builder = Client::builder();
    let mut maybe_proxy_url = instance.proxy_url.as_ref();
    match network {
        Network::Default => (),
        Network::Tor => {
            maybe_proxy_url = instance.onion_proxy_url.as_ref()
                .or(maybe_proxy_url);
        },
        Network::I2p => {
            maybe_proxy_url = instance.i2p_proxy_url.as_ref()
                .or(maybe_proxy_url);
        },
    };
    if let Some(proxy_url) = maybe_proxy_url {
        let proxy = Proxy::all(proxy_url)?;
        client_builder = client_builder.proxy(proxy);
    };
    let request_timeout = Duration::from_secs(timeout);
    let connect_timeout = Duration::from_secs(max(
        timeout,
        CONNECTION_TIMEOUT,
    ));
    client_builder
        .timeout(request_timeout)
        .connect_timeout(connect_timeout)
        .build()
}

// Workaround for https://github.com/seanmonstar/reqwest/issues/1234
pub async fn limited_response(
    mut response: Response,
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
