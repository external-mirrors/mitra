use std::net::{IpAddr, Ipv4Addr};
use std::str::FromStr;

use actix_governor::{KeyExtractor, SimpleKeyExtractionError};
use actix_web::dev::ServiceRequest;

// Based on https://github.com/AaronErhardt/actix-governor/blob/v0.8.0/examples/custom_key_ip.rs
#[derive(Clone)]
pub struct RealIpKeyExtractor;

impl KeyExtractor for RealIpKeyExtractor {
    type Key = IpAddr;
    type KeyExtractionError = SimpleKeyExtractionError<&'static str>;

    fn extract(
        &self,
        request: &ServiceRequest,
    ) -> Result<Self::Key, Self::KeyExtractionError> {
        // TODO: make reverse proxy address configurable
        let reverse_proxy_ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        let maybe_peer_ip = request.peer_addr().map(|socket| socket.ip());
        let maybe_real_ip = request.connection_info()
            .realip_remote_addr()
            .and_then(|real_ip| IpAddr::from_str(real_ip).ok());
        match maybe_peer_ip {
            Some(peer_ip) if peer_ip == reverse_proxy_ip => {
                // "real IP" can be trusted only if coming from reverse proxy
                Ok(maybe_real_ip.unwrap_or(peer_ip))
            },
            Some(peer_ip) => Ok(peer_ip),
            None => {
                // Unix socket?
                maybe_real_ip
                    .ok_or(SimpleKeyExtractionError::new("Could not extract real IP address from request"))
            },
        }
    }
}
