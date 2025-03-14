use std::net::IpAddr;

use url::{Host, Url};

pub use url::{
    ParseError as UrlError,
};

use crate::url::hostname::guess_protocol;

/// Returns URL host name (without port number)
/// IDNs are converted into punycode
pub fn get_hostname(url: &str) -> Result<String, UrlError> {
    let hostname = match Url::parse(url)?
        .host()
        .ok_or(UrlError::EmptyHost)?
    {
        Host::Domain(domain) => domain.to_string(),
        Host::Ipv4(addr) => addr.to_string(),
        Host::Ipv6(addr) => addr.to_string(),
    };
    Ok(hostname)
}

pub fn get_ip_address(url: &Url) -> Option<IpAddr> {
    let host = url.host()?;
    match host {
        Host::Domain(_) => None,
        Host::Ipv4(addr) => Some(IpAddr::V4(addr)),
        Host::Ipv6(addr) => Some(IpAddr::V6(addr)),
    }
}

// Normalize HTTP origin:
// - add a scheme if it's missing
// - convert IDN to punycode
pub fn normalize_origin(url: &str) -> Result<String, UrlError> {
    let normalized_url = if
        url.starts_with("http://") ||
        url.starts_with("https://")
    {
        url.to_string()
    } else {
        // Add scheme
        // Doesn't work for IPv6
        let hostname = if let Some((hostname, _port)) = url.split_once(':') {
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
    let url = Url::parse(&normalized_url)?;
    url.host().ok_or(UrlError::EmptyHost)?; // validates URL
    let origin = url.origin().ascii_serialization();
    Ok(origin)
}

#[cfg(test)]
mod tests {
    use std::net::{Ipv4Addr, Ipv6Addr};
    use super::*;

    #[test]
    fn test_get_hostname() {
        let url = "https://example.org/objects/1";
        let hostname = get_hostname(url).unwrap();
        assert_eq!(hostname, "example.org");
    }

    #[test]
    fn test_get_hostname_if_port_number() {
        let url = "http://127.0.0.1:8380/objects/1";
        let hostname = get_hostname(url).unwrap();
        assert_eq!(hostname, "127.0.0.1");
    }

    #[test]
    fn test_get_hostname_tor() {
        let url = "http://2gzyxa5ihm7nsggfxnu52rck2vv4rvmdlkiu3zzui5du4xyclen53wid.onion/objects/1";
        let hostname = get_hostname(url).unwrap();
        assert_eq!(hostname, "2gzyxa5ihm7nsggfxnu52rck2vv4rvmdlkiu3zzui5du4xyclen53wid.onion");
    }

    #[test]
    fn test_get_hostname_yggdrasil() {
        let url = "http://[319:3cf0:dd1d:47b9:20c:29ff:fe2c:39be]/objects/1";
        let hostname = get_hostname(url).unwrap();
        assert_eq!(hostname, "319:3cf0:dd1d:47b9:20c:29ff:fe2c:39be");
    }

    #[test]
    fn test_get_hostname_idn() {
        let url = "https://räksmörgås.josefsson.org/raksmorgas.jpg";
        let hostname = get_hostname(url).unwrap();
        assert_eq!(hostname, "xn--rksmrgs-5wao1o.josefsson.org");
    }

    #[test]
    fn test_get_hostname_email() {
        let url = "mailto:user@example.org";
        let result = get_hostname(url);
        assert_eq!(result.is_err(), true);
    }

    #[test]
    fn test_get_ip_address() {
        let url = Url::parse("https://server.example/test").unwrap();
        assert_eq!(get_ip_address(&url), None);

        let url = Url::parse("http://127.0.0.1:5941/test").unwrap();
        assert_eq!(
            get_ip_address(&url),
            Some(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))),
        );

        let url = Url::parse("http://[::1]:5941/test").unwrap();
        assert_eq!(
            get_ip_address(&url),
            Some(IpAddr::V6(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1))),
        );
    }

    #[test]
    fn test_get_ip_address_invalid_ip() {
        let url = Url::parse("https://127:5941/test").unwrap();
        assert_eq!(url.host_str().unwrap(), "0.0.0.127");
        assert_eq!(
            get_ip_address(&url),
            Some(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 127))),
        );
    }

    #[test]
    fn test_normalize_origin() {
        let output = normalize_origin("https://social.example").unwrap();
        assert_eq!(output, "https://social.example");
        let output = normalize_origin("social.example").unwrap();
        assert_eq!(output, "https://social.example");
        // IDN
        let output = normalize_origin("嘟文.com").unwrap();
        assert_eq!(output, "https://xn--j5r817a.com");
        // IP address
        let output = normalize_origin("127.0.0.1:8380").unwrap();
        assert_eq!(output, "http://127.0.0.1:8380");
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
}
