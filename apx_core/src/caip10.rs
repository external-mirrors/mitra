/// https://github.com/ChainAgnostic/CAIPs/blob/master/CAIPs/caip-10.md
use std::fmt;
use std::str::FromStr;

use regex::Regex;

use super::caip2::{ChainId, CAIP2_RE};

const CAIP10_ADDRESS_RE: &str = r"(?P<address>[-.%a-zA-Z0-9]{1,128})";

// 'caip:' URI scheme has not been standardized
// https://github.com/ChainAgnostic/CAIPs/issues/67
const CAIP10_URI_PREFIX: &str = "caip:10:";

#[derive(Clone, Debug, PartialEq)]
pub struct AccountId {
    pub chain_id: ChainId,
    pub address: String,
}

#[derive(thiserror::Error, Debug)]
#[error("invalid CAIP-10 ID")]
pub struct AccountIdError;

impl AccountId {
    pub fn to_uri(&self) -> String {
        format!("{}{}", CAIP10_URI_PREFIX, self)
    }

    pub fn from_uri(uri: &str) -> Result<Self, AccountIdError> {
        let account_id_str = uri.strip_prefix(CAIP10_URI_PREFIX)
            .ok_or(AccountIdError)?;
        let account_id = account_id_str.parse()?;
        Ok(account_id)
    }
}

impl fmt::Display for AccountId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}", self.chain_id, self.address)
    }
}

impl FromStr for AccountId {
    type Err = AccountIdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let caip10_re_str = format!("{}:{}", CAIP2_RE, CAIP10_ADDRESS_RE);
        let caip10_re = Regex::new(&caip10_re_str)
            .expect("regexp should be valid");
        let caps = caip10_re.captures(value).ok_or(AccountIdError)?;
        let chain_id = ChainId::new(&caps["namespace"], &caps["reference"])
            .map_err(|_| AccountIdError)?;
        let account_id = Self {
            chain_id: chain_id,
            address: caps["address"].to_string(),
        };
        Ok(account_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_account_id() {
        let value = "eip155:31337:0xb9c5714089478a327f09197987f16f9e5d936e8a";
        let account_id = value.parse::<AccountId>().unwrap();
        assert_eq!(account_id.chain_id.is_ethereum(), true);
        assert_eq!(account_id.to_string(), value);
    }

    #[test]
    fn test_parse_invalid_account_id() {
        let value = "eip155:0xb9c5714089478a327f09197987f16f9e5d936e8a";
        let error = value.parse::<AccountId>().err().unwrap();
        assert_eq!(error.to_string(), "invalid CAIP-10 ID");
    }

    #[test]
    fn test_account_id_uri() {
        let value = "monero:418015bb9ae982a1975da7d79277c270:8xyz";
        let account_id = value.parse::<AccountId>().unwrap();
        let account_uri = account_id.to_uri();
        assert_eq!(
            account_uri,
            "caip:10:monero:418015bb9ae982a1975da7d79277c270:8xyz",
        );
        let account_id_parsed = AccountId::from_uri(&account_uri).unwrap();
        assert_eq!(account_id_parsed, account_id);
    }
}
