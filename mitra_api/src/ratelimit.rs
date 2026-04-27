use std::net::{IpAddr, Ipv4Addr};
use std::str::FromStr;

use actix_governor::{
    governor::middleware::NoOpMiddleware,
    GovernorConfig,
    GovernorConfigBuilder,
    KeyExtractor,
    SimpleKeyExtractionError,
};
use actix_web::dev::ServiceRequest;

// Based on https://github.com/AaronErhardt/actix-governor/blob/v0.8.0/examples/custom_key_ip.rs
#[derive(Clone, Copy)]
pub struct RealIpKeyExtractor {
    behind_reverse_proxy: bool,
}

impl KeyExtractor for RealIpKeyExtractor {
    type Key = IpAddr;
    type KeyExtractionError = SimpleKeyExtractionError<&'static str>;

    fn extract(
        &self,
        request: &ServiceRequest,
    ) -> Result<Self::Key, Self::KeyExtractionError> {
        // TODO: make reverse proxy address configurable
        let reverse_proxy_ip = self.behind_reverse_proxy
            .then_some(IpAddr::V4(Ipv4Addr::LOCALHOST));
        // Peer address is not known when unix socket is used
        let maybe_peer_ip = request.peer_addr().map(|socket| socket.ip());
        let maybe_real_ip = request.connection_info()
            .realip_remote_addr()
            .and_then(|real_ip| IpAddr::from_str(real_ip).ok());
        let key = match reverse_proxy_ip {
            Some(reverse_proxy_ip) => {
                // Use proxy IP if peer address is not known
                let peer_ip = maybe_peer_ip.unwrap_or(reverse_proxy_ip);
                if peer_ip == reverse_proxy_ip {
                    // "real IP" can be trusted only if coming from reverse proxy
                    maybe_real_ip.unwrap_or(peer_ip)
                } else {
                    peer_ip
                }
            },
            None => {
                // Use localhost if peer address is not known
                maybe_peer_ip.unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST))
            },
        };
        Ok(key)
    }
}

pub type RatelimitConfig = GovernorConfig<RealIpKeyExtractor, NoOpMiddleware>;
pub use actix_governor::Governor;

fn ratelimit_config(
    extractor: RealIpKeyExtractor,
    num_requests: u32,
    period: u64,
    permissive: bool,
) -> RatelimitConfig {
    GovernorConfigBuilder::default()
        .key_extractor(extractor)
        .burst_size(num_requests)
        .seconds_per_request(period)
        .permissive(permissive)
        .finish()
        .expect("governor parameters should be non-zero")
}

#[derive(Clone)]
pub struct RatelimitConfigs {
    pub registration: RatelimitConfig,
    pub login: RatelimitConfig,
    pub search: RatelimitConfig,
}

impl RatelimitConfigs {
    pub fn new(behind_reverse_proxy: bool) -> Self {
        let extractor = RealIpKeyExtractor { behind_reverse_proxy };
        Self {
            registration: ratelimit_config(extractor, 2, 300, false),
            login: ratelimit_config(extractor, 5, 120, false),
            search: ratelimit_config(extractor, 2, 30, true),
        }
    }
}
