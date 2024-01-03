use std::{fmt, str::FromStr};

use regex::Regex;
use serde::Deserialize;

use mitra_models::profiles::types::DbActorProfile;
use mitra_validators::errors::ValidationError;

// See also: USERNAME_RE in mitra_validators::profiles
const ACTOR_ADDRESS_RE: &str = r"^(?P<username>[\w\.-]+)@(?P<hostname>[\w\.-]+)$";

#[derive(Deserialize)]
pub struct WebfingerQueryParams {
    pub resource: String,
}

#[derive(Eq, Ord, PartialEq, PartialOrd)]
pub struct ActorAddress {
    pub username: String,
    pub hostname: String, // does not include port number
}

impl ActorAddress {
    pub fn from_handle(
        handle: &str,
    ) -> Result<Self, ValidationError> {
        // @ prefix is optional
        let actor_address = handle.strip_prefix('@')
            .unwrap_or(handle)
            .parse()?;
        Ok(actor_address)
    }

    pub fn from_profile(
        local_hostname: &str,
        profile: &DbActorProfile,
    ) -> Self {
        assert_eq!(profile.hostname.is_none(), profile.is_local());
        Self {
            username: profile.username.clone(),
            hostname: profile.hostname.as_deref()
                .unwrap_or(local_hostname)
                .to_string(),
        }
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

    pub(super) fn from_acct_uri(uri: &str) -> Result<Self, ValidationError> {
        let actor_address = uri.strip_prefix("acct:")
            .ok_or(ValidationError("invalid acct: URI"))?
            .parse()?;
        Ok(actor_address)
    }
}

impl FromStr for ActorAddress {
    type Err = ValidationError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let actor_address_re = Regex::new(ACTOR_ADDRESS_RE).unwrap();
        let caps = actor_address_re.captures(value)
            .ok_or(ValidationError("invalid actor address"))?;
        let actor_address = Self {
            username: caps["username"].to_string(),
            hostname: caps["hostname"].to_string(),
        };
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
    use mitra_models::profiles::types::DbActor;
    use super::*;

    #[test]
    fn test_local_actor_address() {
        let local_hostname = "example.com";
        let local_profile = DbActorProfile {
            username: "user".to_string(),
            hostname: None,
            acct: "user".to_string(),
            actor_json: None,
            ..Default::default()
        };
        let actor_address = ActorAddress::from_profile(
            local_hostname,
            &local_profile,
        );
        assert_eq!(
            actor_address.to_string(),
            "user@example.com",
        );
        assert_eq!(
            actor_address.acct(local_hostname),
            local_profile.acct,
        );
    }

    #[test]
    fn test_remote_actor_address() {
        let local_hostname = "example.com";
        let remote_profile = DbActorProfile {
            username: "test".to_string(),
            hostname: Some("remote.com".to_string()),
            acct: "test@remote.com".to_string(),
            actor_json: Some(DbActor {
                id: "https://test".to_string(),
                ..Default::default()
            }),
            ..Default::default()
        };
        let actor_address = ActorAddress::from_profile(
            local_hostname,
            &remote_profile,
        );
        assert_eq!(
            actor_address.to_string(),
            remote_profile.acct,
        );
        assert_eq!(
            actor_address.acct(local_hostname),
            remote_profile.acct,
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
        let address_1 = ActorAddress::from_handle(handle).unwrap();
        assert_eq!(address_1.acct("example.com"), "user");

        let address_2 = ActorAddress::from_handle(handle).unwrap();
        assert_eq!(address_2.acct("server.info"), "user@example.com");

        let handle_without_prefix = "user@test.com";
        let address_3 = ActorAddress::from_handle(handle_without_prefix).unwrap();
        assert_eq!(address_3.to_string(), handle_without_prefix);

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
