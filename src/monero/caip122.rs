/// https://github.com/ChainAgnostic/CAIPs/blob/master/CAIPs/caip-122.md
use std::str::FromStr;

use chrono::{DateTime, Utc};
use monero_rpc::monero::{
    network::Network,
    util::address::Error as AddressError,
};

use mitra_config::MoneroConfig;
use mitra_utils::{
    caip10::AccountId,
    caip2::{ChainId, MoneroNetwork},
};

use super::utils::parse_monero_address;
use super::wallet::{verify_monero_signature, MoneroError};

const PREAMBLE: &str = " wants you to sign in with your Monero account:";
const URI_TAG: &str = "URI: ";
const VERSION_TAG: &str = "Version: ";
const NONCE_TAG: &str = "Nonce: ";
const ISSUED_AT_TAG: &str = "Issued At: ";
const EXPIRATION_TIME_TAG: &str = "Expiration Time: ";
const NOT_BEFORE_TAG: &str = "Not Before: ";
const REQUEST_ID_TAG: &str = "Request ID: ";
const RESOURCES_TAG: &str = "Resources:";

#[allow(dead_code)]
struct Caip122Message {
    domain: String,
    address: String,
    uri: String,
    version: String,
    statement: Option<String>,
    nonce: String,
    issued_at: Option<DateTime<Utc>>,
    expiration_time: Option<DateTime<Utc>>,
    not_before: Option<DateTime<Utc>>,
    request_id: Option<String>,
    resources: Vec<String>,
}

#[derive(thiserror::Error, Debug)]
pub enum Caip122Error {
    #[error("{0}")]
    InvalidMessage(&'static str),

    #[error("invalid date")]
    InvalidDate(#[from] chrono::ParseError),

    #[error(transparent)]
    AddressError(#[from] AddressError),

    #[error(transparent)]
    SignatureError(#[from] MoneroError),
}

// Message structure is similar to EIP-4361 message
// https://eips.ethereum.org/EIPS/eip-4361
impl FromStr for Caip122Message {
    type Err = Caip122Error;

    fn from_str(message_str: &str) -> Result<Self, Self::Err> {
        let mut lines = message_str.split('\n');
        let domain = lines.next()
            .and_then(|preamble| preamble.strip_suffix(PREAMBLE))
            .ok_or(Caip122Error::InvalidMessage("missing preamble line"))?;
        let address = lines.next()
            .ok_or(Caip122Error::InvalidMessage("missing address"))?;
        lines.next(); // empty line before statement
        let maybe_statement = match lines.next() {
            None => return Err(Caip122Error::InvalidMessage("no lines found after address")),
            Some("") => None,
            Some(statement) => {
                lines.next(); // empty line after statement
                Some(statement.to_string())
            },
        };
        let uri = lines.next()
            .and_then(|line| line.strip_prefix(URI_TAG))
            .ok_or(Caip122Error::InvalidMessage("missing URI"))?;
        let version = lines.next()
            .and_then(|line| line.strip_prefix(VERSION_TAG))
            .ok_or(Caip122Error::InvalidMessage("missing version"))?;
        let nonce = lines.next()
            .and_then(|line| line.strip_prefix(NONCE_TAG))
            .ok_or(Caip122Error::InvalidMessage("missing nonce"))?;

        let mut maybe_line = lines.next();
        let maybe_issued_at = maybe_line
            .and_then(|line| line.strip_prefix(ISSUED_AT_TAG))
            .map(DateTime::parse_from_rfc3339)
            .transpose()?
            .map(|date| date.with_timezone(&Utc));
        if maybe_issued_at.is_some() {
            maybe_line = lines.next();
        };
        let maybe_expiration_time = maybe_line
            .and_then(|line| line.strip_prefix(EXPIRATION_TIME_TAG))
            .map(DateTime::parse_from_rfc3339)
            .transpose()?
            .map(|date| date.with_timezone(&Utc));
        if maybe_expiration_time.is_some() {
            maybe_line = lines.next();
        };
        let maybe_not_before = maybe_line
            .and_then(|line| line.strip_prefix(NOT_BEFORE_TAG))
            .map(DateTime::parse_from_rfc3339)
            .transpose()?
            .map(|date| date.with_timezone(&Utc));
        if maybe_not_before.is_some() {
            maybe_line = lines.next();
        };
        let maybe_request_id = maybe_line
            .and_then(|line| line.strip_prefix(REQUEST_ID_TAG))
            .map(|value| value.to_string());
        if maybe_request_id.is_some() {
            maybe_line = lines.next();
        };
        let mut resources = vec![];
        if maybe_line == Some(RESOURCES_TAG) {
            for line in lines {
                let resource = line.strip_prefix("- ")
                    .ok_or(Caip122Error::InvalidMessage("invalid resource"))?
                    .to_string();
                resources.push(resource);
            };
        };

        let message = Self {
            domain: domain.to_string(),
            address: address.to_string(),
            uri: uri.to_string(),
            version: version.to_string(),
            statement: maybe_statement,
            nonce: nonce.to_string(),
            issued_at: maybe_issued_at,
            expiration_time: maybe_expiration_time,
            not_before: maybe_not_before,
            request_id: maybe_request_id,
            resources: resources,
        };
        Ok(message)
    }
}

impl Caip122Message {
    pub fn valid_now(&self) -> bool {
        let now = Utc::now();
        let nbf_valid = self.not_before.as_ref()
            .map(|nbf| nbf < &now).unwrap_or(true);
        let exp_valid = self.expiration_time.as_ref()
            .map(|exp| exp >= &now).unwrap_or(true);
        nbf_valid && exp_valid
    }
}

pub struct Caip122SessionData {
    pub account_id: AccountId,
    pub nonce: String,
}

pub async fn verify_monero_caip122_signature(
    config: &MoneroConfig,
    instance_hostname: &str,
    login_message: &str,
    message_str: &str,
    signature: &str,
) -> Result<Caip122SessionData, Caip122Error> {
    let message: Caip122Message = message_str.parse()?;
    if message.domain != instance_hostname {
        return Err(Caip122Error::InvalidMessage("domain doesn't match instance hostname"));
    };
    let statement = message.statement.as_ref()
        .ok_or(Caip122Error::InvalidMessage("statement is missing"))?;
    if statement != login_message {
        return Err(Caip122Error::InvalidMessage("statement doesn't match login message"));
    };
    if !message.valid_now() {
        return Err(Caip122Error::InvalidMessage("message is not currently valid"));
    };
    verify_monero_signature(
        config,
        &message.address,
        message_str,
        signature,
    ).await?;
    let address = parse_monero_address(&message.address)?;
    let network = match address.network {
        Network::Mainnet => MoneroNetwork::Mainnet,
        Network::Stagenet => MoneroNetwork::Stagenet,
        Network::Testnet => MoneroNetwork::Testnet,
    };
    let chain_id = ChainId::from_monero_network(network);
    let session_data = Caip122SessionData {
        account_id: AccountId {
            chain_id,
            address: address.to_string(),
        },
        nonce: message.nonce,
    };
    Ok(session_data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_caip122_message() {
        let message_str = r#"example.org wants you to sign in with your Monero account:
888tNkZrPN6JsEgekjMnABU4TBzc2Dt29EPAvkRxbANsAnjyPbb3iQ1YBRk1UXcdRsiKc9dhwMVgN5S9cQUiyoogDavup3H

Hello

URI: https://example.org
Version: 1
Nonce: deadbeef
Issued At: 2023-05-01T03:25:28Z
Expiration Time: 2023-05-03T08:25:28Z
Not Before: 2023-05-01T01:25:28Z
Request ID: test
Resources:
- https://example.org/res1
- https://example.org/res2"#;
        let message: Caip122Message = message_str.parse().unwrap();
        assert_eq!(message.domain, "example.org");
        assert_eq!(message.address, "888tNkZrPN6JsEgekjMnABU4TBzc2Dt29EPAvkRxbANsAnjyPbb3iQ1YBRk1UXcdRsiKc9dhwMVgN5S9cQUiyoogDavup3H");
        assert_eq!(message.uri, "https://example.org");
        assert_eq!(message.version, "1");
        assert_eq!(message.statement.unwrap(), "Hello");
        assert_eq!(message.nonce, "deadbeef");
        assert_eq!(
            message.issued_at.unwrap().to_rfc3339(),
            "2023-05-01T03:25:28+00:00",
        );
        assert_eq!(
            message.expiration_time.unwrap().to_rfc3339(),
            "2023-05-03T08:25:28+00:00",
        );
        assert_eq!(
            message.not_before.unwrap().to_rfc3339(),
            "2023-05-01T01:25:28+00:00",
        );
        assert_eq!(message.request_id.unwrap(), "test");
        assert_eq!(message.resources, vec![
            "https://example.org/res1",
            "https://example.org/res2",
        ]);
    }

    #[test]
    fn test_parse_caip122_message_minimal() {
        let message_str = r#"example.org wants you to sign in with your Monero account:
888tNkZrPN6JsEgekjMnABU4TBzc2Dt29EPAvkRxbANsAnjyPbb3iQ1YBRk1UXcdRsiKc9dhwMVgN5S9cQUiyoogDavup3H


URI: https://example.org
Version: 1
Nonce: ffd6db134"#;
        let message: Caip122Message = message_str.parse().unwrap();
        assert_eq!(message.domain, "example.org");
        assert_eq!(message.address, "888tNkZrPN6JsEgekjMnABU4TBzc2Dt29EPAvkRxbANsAnjyPbb3iQ1YBRk1UXcdRsiKc9dhwMVgN5S9cQUiyoogDavup3H");
        assert_eq!(message.uri, "https://example.org");
        assert_eq!(message.version, "1");
        assert_eq!(message.statement, None);
        assert_eq!(message.nonce, "ffd6db134");
        assert_eq!(message.issued_at, None);
        assert_eq!(message.expiration_time, None);
        assert_eq!(message.not_before, None);
        assert_eq!(message.request_id, None);
        assert_eq!(message.resources.is_empty(), true);
    }
}
