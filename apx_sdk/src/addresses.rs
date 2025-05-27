//! WebFinger addresses
use std::{fmt, str::FromStr};

use regex::Regex;
use thiserror::Error;

use apx_core::url::hostname::guess_protocol;

// https://swicg.github.io/activitypub-webfinger/#names
// username: RFC-3986 unreserved plus % for percent encoding; case-sensitive
// hostname: normalized (ASCII) or IP literals
//   https://datatracker.ietf.org/doc/html/rfc3986#section-3.2.2
const WEBFINGER_ADDRESS_RE: &str = r"^(?P<username>[A-Za-z0-9\-\._~%]+)@(?P<hostname>[a-z0-9\.-]+|[0-9\.]+|\[[0-9a-f:]+\])$";

#[derive(Debug, Error)]
#[error("{0}")]
pub struct WebfingerAddressError(&'static str);

impl WebfingerAddressError {
    pub fn message(&self) -> &'static str { self.0 }
}

#[derive(Eq, Ord, PartialEq, PartialOrd)]
pub struct WebfingerAddress {
    username: String,
    hostname: String, // does not include port number
}

impl WebfingerAddress {
    // Does not validate username and hostname
    pub fn new_unchecked(username: &str, hostname: &str) -> Self {
        Self {
            username: username.to_string(),
            hostname: hostname.to_string(),
        }
    }

    pub fn username(&self) -> &str {
        &self.username
    }

    pub fn hostname(&self) -> &str {
        &self.hostname
    }

    pub fn from_handle(
        handle: &str,
    ) -> Result<Self, WebfingerAddressError> {
        // @ prefix is optional
        let address = handle.strip_prefix('@')
            .unwrap_or(handle)
            .parse()?;
        Ok(address)
    }

    pub fn handle(&self) -> String {
        format!("@{}", self)
    }

    /// Returns 'acct' string (short address).
    /// Used in Mastodon API.
    pub fn acct(&self, local_hostname: &str) -> String {
        if self.hostname == local_hostname {
            self.username.clone()
        } else {
            self.to_string()
        }
    }

    // https://datatracker.ietf.org/doc/html/rfc7565#section-7
    pub fn to_acct_uri(&self) -> String {
        format!("acct:{}", self)
    }

    pub fn from_acct_uri(
        uri: &str,
    ) -> Result<Self, WebfingerAddressError> {
        let address = uri.strip_prefix("acct:")
            .ok_or(WebfingerAddressError("invalid acct: URI"))?
            .parse()?;
        Ok(address)
    }

    /// Returns WebFinger endpoint URI  
    /// <https://datatracker.ietf.org/doc/html/rfc7033#section-4>
    pub fn endpoint_uri(&self) -> String {
        format!(
            "{}://{}/.well-known/webfinger",
            guess_protocol(self.hostname()),
            self.hostname(),
        )
    }
}

impl FromStr for WebfingerAddress {
    type Err = WebfingerAddressError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let address_re = Regex::new(WEBFINGER_ADDRESS_RE)
            .expect("regexp should be valid");
        let caps = address_re.captures(value)
            .ok_or(WebfingerAddressError("invalid webfinger address"))?;
        let address = Self::new_unchecked(
            &caps["username"],
            &caps["hostname"],
        );
        Ok(address)
    }
}

impl fmt::Display for WebfingerAddress {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}@{}", self.username, self.hostname)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_local_address() {
        let local_hostname = "local.example";
        let address = WebfingerAddress::new_unchecked(
            "user",
            local_hostname,
        );
        assert_eq!(
            address.to_string(),
            "user@local.example",
        );
        assert_eq!(
            address.acct(local_hostname),
            "user",
        );
    }

    #[test]
    fn test_remote_address() {
        let local_hostname = "local.example";
        let address = WebfingerAddress::new_unchecked(
            "user",
            "remote.example",
        );
        assert_eq!(
            address.to_string(),
            "user@remote.example",
        );
        assert_eq!(
            address.acct(local_hostname),
            "user@remote.example",
        );
    }

    #[test]
    fn test_address_parse() {
        let value = "user_1@example.com";
        let address = value.parse::<WebfingerAddress>().unwrap();
        assert_eq!(address.username, "user_1");
        assert_eq!(address.hostname, "example.com");
        assert_eq!(address.to_string(), value);
    }

    #[test]
    fn test_address_parse_percent_encoded() {
        let value = "did%3Aexample%3A12-34@social.example";
        let address = value.parse::<WebfingerAddress>().unwrap();
        assert_eq!(address.username, "did%3Aexample%3A12-34");
        assert_eq!(address.hostname, "social.example");
        assert_eq!(address.to_string(), value);
    }

    #[test]
    fn test_address_parse_ipv4() {
        let value = "admin@127.0.0.1";
        let address = value.parse::<WebfingerAddress>().unwrap();
        assert_eq!(address.username, "admin");
        assert_eq!(address.hostname, "127.0.0.1");
        assert_eq!(address.to_string(), value);
    }

    #[test]
    fn test_address_parse_ipv6() {
        let value = "admin@[319:3cf0:dd1d:47b9:20c:29ff:fe2c:39be]";
        let address = value.parse::<WebfingerAddress>().unwrap();
        assert_eq!(address.username, "admin");
        assert_eq!(address.hostname, "[319:3cf0:dd1d:47b9:20c:29ff:fe2c:39be]");
        assert_eq!(address.to_string(), value);
    }

    #[test]
    fn test_parse_unicode_username() {
        let value = "δοκιμή@social.example";
        let error = value.parse::<WebfingerAddress>().err().unwrap();
        assert_eq!(error.0, "invalid webfinger address");
    }

    #[test]
    fn test_address_parse_idn() {
        let value = "user_1@bücher.example";
        let error = value.parse::<WebfingerAddress>().err().unwrap();
        assert_eq!(error.0, "invalid webfinger address");
    }

    #[test]
    fn test_address_parse_ipv4_with_port() {
        let value = "admin@127.0.0.1:8000";
        let error = value.parse::<WebfingerAddress>().err().unwrap();
        assert_eq!(error.0, "invalid webfinger address");
    }

    #[test]
    fn test_address_parse_ipv6_no_brackets() {
        let value = "admin@319:3cf0:dd1d:47b9:20c:29ff:fe2c:39be";
        let error = value.parse::<WebfingerAddress>().err().unwrap();
        assert_eq!(error.0, "invalid webfinger address");
    }

    #[test]
    fn test_address_parse_handle() {
        let handle = "@user_1@example.com";
        let result = handle.parse::<WebfingerAddress>();
        assert_eq!(result.is_err(), true);
    }

    #[test]
    fn test_address_from_handle() {
        let handle = "@user@example.com";
        let address = WebfingerAddress::from_handle(handle).unwrap();
        assert_eq!(address.to_string(), "user@example.com");

        // Prefix can be removed only once
        let handle = "@@user@example.com";
        let result = WebfingerAddress::from_handle(handle);
        assert_eq!(result.is_err(), true);

        let handle_without_prefix = "user@test.com";
        let address = WebfingerAddress::from_handle(handle_without_prefix).unwrap();
        assert_eq!(address.to_string(), handle_without_prefix);

        let short_handle = "@user";
        let result = WebfingerAddress::from_handle(short_handle);
        assert_eq!(result.is_err(), true);
    }

    #[test]
    fn test_address_acct_uri() {
        let uri = "acct:user_1@example.com";
        let address = WebfingerAddress::from_acct_uri(uri).unwrap();
        assert_eq!(address.username, "user_1");
        assert_eq!(address.hostname, "example.com");
        assert_eq!(address.to_acct_uri(), uri);
    }

    #[test]
    fn test_address_acct_uri_unicode() {
        // Hostname in 'acct' URI must be encoded
        let uri = "acct:user_1@δοκιμή.example";
        let error = WebfingerAddress::from_acct_uri(uri).err().unwrap();
        assert_eq!(error.0, "invalid webfinger address");
    }

    #[test]
    fn test_address_endpoint_uri() {
        let value = "user_1@social.example";
        let address: WebfingerAddress = value.parse().unwrap();
        let endpoint_uri = address.endpoint_uri();
        assert_eq!(
            endpoint_uri,
            "https://social.example/.well-known/webfinger",
        );
    }
}
