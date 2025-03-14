use std::net::{Ipv4Addr, Ipv6Addr};

use idna::{domain_to_ascii, Errors as IdnaError};

pub fn encode_hostname(hostname: &str) -> Result<String, IdnaError> {
    domain_to_ascii(hostname)
}

pub fn guess_protocol(hostname: &str) -> &'static str {
    if hostname == "localhost" {
        return "http";
    };
    if hostname.parse::<Ipv4Addr>().is_ok() {
        return "http";
    };
    if hostname.parse::<Ipv6Addr>().is_ok() {
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
    fn test_encode_hostname() {
        let hostname = "räksmörgås.josefsson.org";
        let encoded = encode_hostname(hostname).unwrap();
        assert_eq!(encoded, "xn--rksmrgs-5wao1o.josefsson.org");
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
}
