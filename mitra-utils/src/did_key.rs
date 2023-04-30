/// https://w3c-ccg.github.io/did-method-key/
use std::fmt;
use std::str::FromStr;

use regex::Regex;

use super::{
    did::DidParseError,
    multibase::{
        decode_multibase_base58btc,
        encode_multibase_base58btc,
    },
    multicodec::{
        decode_ed25519_public_key,
        encode_ed25519_public_key,
        MulticodecError,
    },
};

const DID_KEY_RE: &str = r"did:key:(?P<key>z[a-km-zA-HJ-NP-Z1-9]+)";

#[derive(Clone, Debug, PartialEq)]
pub struct DidKey {
    pub key: Vec<u8>,
}

impl DidKey {
    pub fn key_multibase(&self) -> String {
        encode_multibase_base58btc(&self.key)
    }

    pub fn from_ed25519_key(key: [u8; 32]) -> Self {
        let prefixed_key = encode_ed25519_public_key(key);
        Self { key: prefixed_key }
    }

    pub fn try_ed25519_key(&self) -> Result<[u8; 32], MulticodecError> {
        decode_ed25519_public_key(&self.key)
    }
}

impl FromStr for DidKey {
    type Err = DidParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let did_key_re = Regex::new(DID_KEY_RE).unwrap();
        let caps = did_key_re.captures(value).ok_or(DidParseError)?;
        let key = decode_multibase_base58btc(&caps["key"])
            .map_err(|_| DidParseError)?;
        let did_key = Self { key };
        Ok(did_key)
    }
}

impl fmt::Display for DidKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let did_str = format!("did:key:{}", self.key_multibase());
        write!(formatter, "{}", did_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_did_key_string_conversion() {
        let did_str = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";
        let did_key: DidKey = did_str.parse().unwrap();
        assert_eq!(did_key.key.len(), 34); // Ed25519 public key
        let decoded_key = did_key.try_ed25519_key().unwrap();
        let did_key = DidKey::from_ed25519_key(decoded_key);
        assert_eq!(did_key.to_string(), did_str);
    }
}
