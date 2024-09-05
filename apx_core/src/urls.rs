use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use idna::{domain_to_ascii, Errors as IdnaError};
use iri_string::percent_encode::PercentEncodedForUri;
use percent_encoding::percent_decode_str;
use url::{Host, Url};

pub use url::{
    ParseError as UrlError,
};

pub fn validate_uri(uri: &str) -> Result<(), UrlError> {
    Url::parse(uri)?;
    Ok(())
}

/// Encode URI path component (RFC-3986).
/// https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/encodeURIComponent#encoding_for_rfc3986
pub fn url_encode(input: &str) -> String {
    PercentEncodedForUri::unreserve(input).to_string()
}

pub fn url_decode(input: &str) -> String {
    let bytes = percent_decode_str(input);
    bytes.decode_utf8_lossy().to_string()
}

pub fn encode_hostname(hostname: &str) -> Result<String, IdnaError> {
    domain_to_ascii(hostname)
}

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

pub fn guess_protocol(hostname: &str) -> &'static str {
    if hostname == "localhost" {
        return "http";
    };
    let maybe_ipv4_address = hostname.parse::<Ipv4Addr>();
    if let Ok(_ipv4_address) = maybe_ipv4_address {
        return "http";
    };
    let maybe_ipv6_address = hostname.parse::<Ipv6Addr>();
    if let Ok(_ipv6_address) = maybe_ipv6_address {
        return "http";
    };
    if hostname.ends_with(".onion") ||
        hostname.ends_with(".i2p") ||
        hostname.ends_with(".loki")
    {
        // Tor / I2P
        "http"
    } else {
        // Use HTTPS by default
        "https"
    }
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
    use super::*;

    #[test]
    fn test_url_encode_decode() {
        let input = "El Niño"; // unicode and space
        let output = url_encode(input);
        assert_eq!(output, "El%20Ni%C3%B1o");
        let decoded = url_decode(&output);
        assert_eq!(decoded, input);
    }

    #[test]
    fn test_url_encode_decode_reserved_characters() {
        let input_1 = ";/?:@&=+$,#"; // encoded by encodeURIComponent()
        let encoded_1 = url_encode(input_1);
        assert_eq!(encoded_1, "%3B%2F%3F%3A%40%26%3D%2B%24%2C%23");
        let decoded_1 = url_decode(&encoded_1);
        assert_eq!(decoded_1, input_1);

        let input_2 = "-.!~*'()"; // not encoded by encodeURIComponent()
        let encoded_2 = url_encode(input_2);
        assert_eq!(encoded_2, "-.%21~%2A%27%28%29");
        let decoded_2 = url_decode(&encoded_2);
        assert_eq!(decoded_2, input_2);
    }

    #[test]
    fn test_url_encode_decode_url() {
        let input = "https://social.example/users/test_user";
        let output = url_encode(input);
        assert_eq!(output, "https%3A%2F%2Fsocial.example%2Fusers%2Ftest_user");
        let decoded = url_decode(&output);
        assert_eq!(decoded, input);
    }

    #[test]
    fn test_encode_hostname() {
        let hostname = "räksmörgås.josefsson.org";
        let encoded = encode_hostname(hostname).unwrap();
        assert_eq!(encoded, "xn--rksmrgs-5wao1o.josefsson.org");
    }

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
    fn test_guess_protocol() {
        assert_eq!(
            guess_protocol("example.org"),
            "https",
        );
        assert_eq!(
            guess_protocol("2gzyxa5ihm7nsggfxnu52rck2vv4rvmdlkiu3zzui5du4xyclen53wid.onion"),
            "http",
        );
        assert_eq!(
            guess_protocol("zzz.i2p"),
            "http",
        );
        // Yggdrasil
        assert_eq!(
            guess_protocol("319:3cf0:dd1d:47b9:20c:29ff:fe2c:39be"),
            "http",
        );
        // localhost
        assert_eq!(
            guess_protocol("127.0.0.1"),
            "http",
        );
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
