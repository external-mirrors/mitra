/// https://w3c-ccg.github.io/did-method-key/
use std::fmt;
use std::str::FromStr;

use regex::Regex;

use crate::{
    crypto_eddsa::{
        ed25519_public_key_from_bytes,
        Ed25519PublicKey,
    },
    did::DidParseError,
    multibase::{
        decode_multibase_base58btc,
        encode_multibase_base58btc,
    },
    multicodec::{Multicodec, MulticodecError},
};

const DID_KEY_RE: &str = r"did:key:(?P<key>z[a-km-zA-HJ-NP-Z1-9]+)";

#[derive(Clone, Debug, PartialEq)]
pub struct DidKey {
    codec: Multicodec,
    key_data: Vec<u8>,
}

impl DidKey {
    pub(super) const METHOD: &'static str = "key";

    pub fn key_multibase(&self) -> String {
        let multidata = self.codec.encode(&self.key_data);
        encode_multibase_base58btc(&multidata)
    }

    pub fn from_ed25519_key(key: &Ed25519PublicKey) -> Self {
        Self {
            codec: Multicodec::Ed25519Pub,
            key_data: key.as_bytes().to_vec(),
        }
    }

    pub fn try_ed25519_key(&self) -> Result<Ed25519PublicKey, MulticodecError> {
        if self.codec != Multicodec::Ed25519Pub {
            return Err(MulticodecError);
        };
        // Encoded data should be valid ed25519 key
        let public_key = ed25519_public_key_from_bytes(&self.key_data)
            .map_err(|_| MulticodecError)?;
        Ok(public_key)
    }
}

impl FromStr for DidKey {
    type Err = DidParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let did_key_re = Regex::new(DID_KEY_RE)
            .expect("regexp should be valid");
        let caps = did_key_re.captures(value).ok_or(DidParseError)?;
        let multidata = decode_multibase_base58btc(&caps["key"])
            .map_err(|_| DidParseError)?;
        let (codec, key_data) = Multicodec::decode(&multidata)
            .map_err(|_| DidParseError)?;
        let did_key = Self { codec, key_data };
        Ok(did_key)
    }
}

impl fmt::Display for DidKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "did:{}:{}",
            Self::METHOD,
            self.key_multibase(),
        )
    }
}

#[cfg(test)]
mod tests {
    use rsa::traits::PublicKeyParts;
    use crate::crypto_rsa::rsa_public_key_from_pkcs1_der;
    use super::*;

    #[test]
    fn test_did_key_string_conversion() {
        let did_str = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";
        let did_key: DidKey = did_str.parse().unwrap();
        assert_eq!(did_key.key_data.len(), 32); // Ed25519 public key
        let decoded_key = did_key.try_ed25519_key().unwrap();
        let did_key = DidKey::from_ed25519_key(&decoded_key);
        assert_eq!(did_key.to_string(), did_str);
    }

    #[test]
    fn test_parse_did_key_rsa() {
        // did:key test vector, RSA 2048-bit
        let did_str = "did:key:z4MXj1wBzi9jUstyPMS4jQqB6KdJaiatPkAtVtGc6bQEQEEsKTic4G7Rou3iBf9vPmT5dbkm9qsZsuVNjq8HCuW1w24nhBFGkRE4cd2Uf2tfrB3N7h4mnyPp1BF3ZttHTYv3DLUPi1zMdkULiow3M1GfXkoC6DoxDUm1jmN6GBj22SjVsr6dxezRVQc7aj9TxE7JLbMH1wh5X3kA58H3DFW8rnYMakFGbca5CB2Jf6CnGQZmL7o5uJAdTwXfy2iiiyPxXEGerMhHwhjTA1mKYobyk2CpeEcmvynADfNZ5MBvcCS7m3XkFCMNUYBS9NQ3fze6vMSUPsNa6GVYmKx2x6JrdEjCk3qRMMmyjnjCMfR4pXbRMZa3i";
        let did_key: DidKey = did_str.parse().unwrap();
        let key = rsa_public_key_from_pkcs1_der(&did_key.key_data).unwrap();
        assert_eq!(key.size() * 8, 2048);
    }
}
