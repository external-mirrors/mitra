use std::net::{Ipv4Addr, Ipv6Addr};

use idna::{domain_to_ascii_cow, AsciiDenyList, Errors as IdnaError};

fn is_ipv4_hostname(hostname: &str) -> bool {
    hostname.parse::<Ipv4Addr>().is_ok()
}

/// Returns `true` if hostname is an IPv6 literal (enclosed in square brackets)
pub fn is_ipv6_hostname(hostname: &str) -> bool {
    hostname.strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .is_some_and(|address| address.parse::<Ipv6Addr>().is_ok())
}

/// Returns the ASCII representation of a host name
pub fn encode_hostname(hostname: &str) -> Result<String, IdnaError> {
    if is_ipv6_hostname(hostname) {
        Ok(hostname.to_string())
    } else {
        domain_to_ascii_cow(hostname.as_bytes(), AsciiDenyList::URL)
            .map(|output| output.to_string())
    }
}

pub fn is_onion(hostname: &str) -> bool {
    hostname.ends_with(".onion")
}

pub fn is_i2p(hostname: &str) -> bool {
    hostname.ends_with(".i2p")
}

/// Returns `true` if two hostnames have a same apex domain
pub fn is_same_apex_domain(
    hostname_1: &str,
    hostname_2: &str,
) -> bool {
    if is_ipv4_hostname(hostname_1) || is_ipv6_hostname(hostname_1) {
        hostname_1 == hostname_2
    } else {
        // reg-name
        let apex_1: Vec<_> = hostname_1.split('.').rev().take(2).collect();
        let apex_2: Vec<_> = hostname_2.split('.').rev().take(2).collect();
        apex_1 == apex_2
    }
}

/// Attempts to guess the URI scheme (http or https) for the given hostname
pub fn guess_protocol(hostname: &str) -> &'static str {
    if hostname == "localhost" {
        return "http";
    };
    if is_ipv4_hostname(hostname) {
        return "http";
    };
    if is_ipv6_hostname(hostname) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_ipv6_hostname() {
        assert!(is_ipv6_hostname("[319:3cf0:dd1d:47b9:20c:29ff:fe2c:39be]"));
        assert!(!is_ipv6_hostname("319:3cf0:dd1d:47b9:20c:29ff:fe2c:39be"));
        assert!(!is_ipv6_hostname(""));
    }

    #[test]
    fn test_encode_hostname() {
        let hostname = "räksmörgås.josefsson.org";
        let encoded = encode_hostname(hostname).unwrap();
        assert_eq!(encoded, "xn--rksmrgs-5wao1o.josefsson.org");

        let reencoded = encode_hostname(&encoded).unwrap();
        assert_eq!(reencoded, encoded);
    }

    #[test]
    fn test_encode_hostname_ipv4() {
        let hostname = "127.0.0.1";
        let encoded = encode_hostname(hostname).unwrap();
        assert_eq!(encoded, hostname);
    }

    #[test]
    fn test_encode_hostname_ipv6() {
        let hostname = "[319:3cf0:dd1d:47b9:20c:29ff:fe2c:39be]";
        let encoded = encode_hostname(hostname).unwrap();
        assert_eq!(encoded, hostname);
    }

    #[test]
    fn test_is_same_apex_domain() {
        let hostname_1 = "mastodon.example.social";
        let hostname_2 = "pleroma.example.social";
        let hostname_3 = "example.social";
        assert_eq!(is_same_apex_domain(hostname_1, hostname_1), true);
        assert_eq!(is_same_apex_domain(hostname_1, hostname_2), true);
        assert_eq!(is_same_apex_domain(hostname_1, hostname_3), true);
    }

    #[test]
    fn test_is_same_apex_domain_tld() {
        let hostname_1 = "example.social";
        let hostname_2 = "social";
        assert_eq!(is_same_apex_domain(hostname_1, hostname_2), false);
    }

    #[test]
    fn test_is_same_apex_domain_ipv4() {
        let hostname_1 = "127.0.0.1";
        let hostname_2 = "0.0.1";
        assert_eq!(is_same_apex_domain(hostname_1, hostname_2), false);
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
            guess_protocol("[319:3cf0:dd1d:47b9:20c:29ff:fe2c:39be]"),
            "http",
        );
        // localhost
        assert_eq!(
            guess_protocol("127.0.0.1"),
            "http",
        );
    }
}
