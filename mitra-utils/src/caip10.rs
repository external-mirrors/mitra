/// https://github.com/ChainAgnostic/CAIPs/blob/master/CAIPs/caip-10.md
use std::fmt;
use std::str::FromStr;

use regex::Regex;

use super::caip2::{ChainId, CAIP2_RE};

const CAIP10_ADDRESS_RE: &str = r"(?P<address>[-.%a-zA-Z0-9]{1,128})";

#[derive(Clone, Debug, PartialEq)]
pub struct AccountId {
    pub chain_id: ChainId,
    pub address: String,
}

impl fmt::Display for AccountId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}", self.chain_id, self.address)
    }
}

#[derive(thiserror::Error, Debug)]
#[error("invalid CAIP-10 ID")]
pub struct AccountIdError;

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
}
