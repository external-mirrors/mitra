/// https://github.com/w3c-ccg/did-pkh/blob/main/did-pkh-method-draft.md
use std::fmt;
use std::str::FromStr;

use super::{
    caip10::AccountId,
    caip2::ChainId,
    currencies::Currency,
    did::DidParseError,
};

#[derive(Clone, Debug, PartialEq)]
pub struct DidPkh {
    account_id: AccountId,
}

impl DidPkh {
    pub fn address(&self) -> String {
        self.account_id.address.clone()
    }

    pub fn from_address(currency: &Currency, address: &str) -> Self {
        let chain_id = match currency {
            Currency::Ethereum => ChainId::ethereum_mainnet(),
            Currency::Monero => unimplemented!(),
        };
        let address = currency.normalize_address(address);
        let account_id = AccountId { chain_id, address };
        Self { account_id }
    }

    pub fn chain_id(&self) -> ChainId {
        self.account_id.chain_id.clone()
    }

    pub fn currency(&self) -> Option<Currency> {
        self.account_id.chain_id.currency()
    }
}

impl fmt::Display for DidPkh {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "did:pkh:{}", self.account_id)
    }
}

impl FromStr for DidPkh {
    type Err = DidParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let account_id_str = value.strip_prefix("did:pkh:")
            .ok_or(DidParseError)?;
        let account_id: AccountId = account_id_str.parse()
            .map_err(|_| DidParseError)?;
        Ok(Self { account_id })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_did_pkh_string_conversion() {
        let address = "0xB9C5714089478a327F09197987f16f9E5d936E8a";
        let ethereum = Currency::Ethereum;
        let did = DidPkh::from_address(&ethereum, address);
        assert_eq!(did.chain_id(), ChainId::ethereum_mainnet());
        assert_eq!(did.currency().unwrap(), ethereum);
        assert_eq!(did.address(), address.to_lowercase());

        let did_str = did.to_string();
        assert_eq!(
            did_str,
            "did:pkh:eip155:1:0xb9c5714089478a327f09197987f16f9e5d936e8a",
        );

        let did: DidPkh = did_str.parse().unwrap();
        assert_eq!(did.address(), address.to_lowercase());
    }

    #[test]
    fn test_parse_invalid_did_pkh() {
        let did_str = "eip155:1:0xb9c5714089478a327f09197987f16f9e5d936e8a";
        let error = did_str.parse::<DidPkh>().err().unwrap();
        assert_eq!(error.to_string(), "DID parse error");
    }
}
