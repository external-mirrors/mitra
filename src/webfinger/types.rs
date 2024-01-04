use std::{fmt, str::FromStr};

use regex::Regex;
use serde::Deserialize;
use thiserror::Error;

// See also: USERNAME_RE in mitra_validators::profiles
const ACTOR_ADDRESS_RE: &str = r"^(?P<username>[\w\.-]+)@(?P<hostname>[\w\.-]+)$";

#[derive(Deserialize)]
pub struct WebfingerQueryParams {
    pub resource: String,
}

#[derive(Debug, Error)]
#[error("{0}")]
pub struct ActorAddressError(&'static str);

impl ActorAddressError {
    pub fn message(&self) -> &'static str { self.0 }
}

#[derive(Eq, Ord, PartialEq, PartialOrd)]
pub struct ActorAddress {
    pub username: String,
    pub hostname: String, // does not include port number
}

impl ActorAddress {
    pub fn new(username: &str, hostname: &str) -> Self {
        Self {
            username: username.to_string(),
            hostname: hostname.to_string(),
        }
    }

    pub fn from_handle(
        handle: &str,
    ) -> Result<Self, ActorAddressError> {
        // @ prefix is optional
        let actor_address = handle.strip_prefix('@')
            .unwrap_or(handle)
            .parse()?;
        Ok(actor_address)
    }

    pub fn handle(&self) -> String {
        format!("@{}", self)
    }

    /// Returns acct string, as used in Mastodon
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
    ) -> Result<Self, ActorAddressError> {
        let actor_address = uri.strip_prefix("acct:")
            .ok_or(ActorAddressError("invalid acct: URI"))?
            .parse()?;
        Ok(actor_address)
    }
}

impl FromStr for ActorAddress {
    type Err = ActorAddressError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let actor_address_re = Regex::new(ACTOR_ADDRESS_RE).unwrap();
        let caps = actor_address_re.captures(value)
            .ok_or(ActorAddressError("invalid actor address"))?;
        let actor_address = Self::new(
            &caps["username"],
            &caps["hostname"],
        );
        Ok(actor_address)
    }
}

impl fmt::Display for ActorAddress {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}@{}", self.username, self.hostname)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_local_actor_address() {
        let local_hostname = "local.example";
        let actor_address = ActorAddress::new(
            "user",
            local_hostname,
        );
        assert_eq!(
            actor_address.to_string(),
            "user@local.example",
        );
        assert_eq!(
            actor_address.acct(local_hostname),
            "user",
        );
    }

    #[test]
    fn test_remote_actor_address() {
        let local_hostname = "local.example";
        let actor_address = ActorAddress::new(
            "user",
            "remote.example",
        );
        assert_eq!(
            actor_address.to_string(),
            "user@remote.example",
        );
        assert_eq!(
            actor_address.acct(local_hostname),
            "user@remote.example",
        );
    }

    #[test]
    fn test_actor_address_parse_address() {
        let value = "user_1@example.com";
        let actor_address: ActorAddress = value.parse().unwrap();
        assert_eq!(actor_address.username, "user_1");
        assert_eq!(actor_address.hostname, "example.com");
        assert_eq!(actor_address.to_string(), value);
    }

    #[test]
    fn test_actor_address_parse_handle() {
        let handle = "@user_1@example.com";
        let result = handle.parse::<ActorAddress>();
        assert_eq!(result.is_err(), true);
    }

    #[test]
    fn test_actor_address_from_handle() {
        let handle = "@user@example.com";
        let address = ActorAddress::from_handle(handle).unwrap();
        assert_eq!(address.to_string(), "user@example.com");

        // Prefix can be removed only once
        let handle = "@@user@example.com";
        let result = ActorAddress::from_handle(handle);
        assert_eq!(result.is_err(), true);

        let handle_without_prefix = "user@test.com";
        let address = ActorAddress::from_handle(handle_without_prefix).unwrap();
        assert_eq!(address.to_string(), handle_without_prefix);

        let short_handle = "@user";
        let result = ActorAddress::from_handle(short_handle);
        assert_eq!(result.is_err(), true);
    }

    #[test]
    fn test_actor_address_acct_uri() {
        let uri = "acct:user_1@example.com";
        let actor_address = ActorAddress::from_acct_uri(uri).unwrap();
        assert_eq!(actor_address.username, "user_1");
        assert_eq!(actor_address.hostname, "example.com");

        assert_eq!(actor_address.to_acct_uri(), uri);
    }
}
