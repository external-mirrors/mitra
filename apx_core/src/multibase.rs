//! Multibase
//!
//! <https://github.com/multiformats/multibase>
use bs58;
use base64ct::{
    Base64UrlUnpadded,
    Encoding,
    Error as Base64Error,
};
use thiserror::Error;

// https://github.com/multiformats/multibase#multibase-table
const BASE_58_BTC_PREFIX: &str = "z";
const BASE_64_URL_NO_PAD_PREFIX: &str = "u";

/// Multibase encodings
pub enum Multibase {
    /// `base-58-btc` alphabet.
    ///
    /// **This encoding is not constant-time**.
    Base58Btc,
    /// `base-64-url-no-pad` alphabet.
    Base64UrlNoPad,
}

/// Errors that may occur when decoding multibase strings
#[derive(Debug, Error)]
pub enum MultibaseError {
    #[error("invalid base string")]
    InvalidBaseString,

    #[error("unknown base")]
    UnknownBase,

    #[error(transparent)]
    Base58Error(#[from] bs58::decode::Error),

    #[error(transparent)]
    Base64Error(#[from] Base64Error),
}

impl Multibase {
    /// Encodes bytes to a multibase string
    pub fn encode(self, value: &[u8]) -> String {
        match self {
            Self::Base58Btc => {
                let encoded = bs58::encode(value)
                    .with_alphabet(bs58::Alphabet::BITCOIN)
                    .into_string();
                format!("{BASE_58_BTC_PREFIX}{encoded}")
            },
            Self::Base64UrlNoPad => {
                let encoded = Base64UrlUnpadded::encode_string(value);
                format!("{BASE_64_URL_NO_PAD_PREFIX}{encoded}")
            },
        }
    }

    /// Decodes a multibase string
    pub fn decode(value: &str) -> Result<(Self, Vec<u8>), MultibaseError> {
        let prefix = value.chars().next()
            .ok_or(MultibaseError::InvalidBaseString)?;
        let encoded_data = &value[prefix.len_utf8()..];
        let output = match prefix.to_string().as_str() {
            BASE_58_BTC_PREFIX => {
                let data = bs58::decode(encoded_data)
                    .with_alphabet(bs58::Alphabet::BITCOIN)
                    .into_vec()?;
                (Self::Base58Btc, data)
            },
            BASE_64_URL_NO_PAD_PREFIX => {
                let data = Base64UrlUnpadded::decode_vec(encoded_data)?;
                (Self::Base64UrlNoPad, data)
            },
            _ => return Err(MultibaseError::UnknownBase),
        };
        Ok(output)
    }

    /// Decodes a multibase string using the specified encoding
    pub fn decode_exact(self, value: &str) -> Result<Vec<u8>, MultibaseError> {
        let (_encoding, data) = Self::decode(value)?;
        if !matches!(self, _encoding) {
            return Err(MultibaseError::UnknownBase);
        };
        Ok(data)
    }
}

/// Decodes multibase base58 (bitcoin) value
pub fn decode_multibase_base58btc(value: &str)
    -> Result<Vec<u8>, MultibaseError>
{
    Multibase::Base58Btc.decode_exact(value)
}

pub fn encode_multibase_base58btc(value: &[u8]) -> String {
    Multibase::Base58Btc.encode(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multibase_test_vectors() {
        // https://github.com/multiformats/multibase/tree/master/tests
        let result = encode_multibase_base58btc("yes mani !".as_bytes());
        assert_eq!(result, "z7paNL19xttacUY");
        let value = decode_multibase_base58btc("z7paNL19xttacUY").unwrap();
        assert_eq!(value, "yes mani !".as_bytes());

        let result = encode_multibase_base58btc("\x00yes mani !".as_bytes());
        assert_eq!(result, "z17paNL19xttacUY");
        let result = encode_multibase_base58btc("\x00\x00yes mani !".as_bytes());
        assert_eq!(result, "z117paNL19xttacUY");
        let result = encode_multibase_base58btc("yes mani !".as_bytes());
        assert_eq!(result, "z7paNL19xttacUY");

        let result = Multibase::Base64UrlNoPad.encode("yes mani !".as_bytes());
        assert_eq!(result, "ueWVzIG1hbmkgIQ");
    }

    #[test]
    fn test_base58btc_encode_decode() {
        let value = [1; 20];
        let encoded = encode_multibase_base58btc(&value);
        let decoded = decode_multibase_base58btc(&encoded).unwrap();
        assert_eq!(decoded, value);
    }

    #[test]
    fn test_base64_url_nopad_encode_decode() {
        let value = [1; 20];
        let encoded = Multibase::Base64UrlNoPad.encode(&value);
        let decoded = Multibase::Base64UrlNoPad.decode_exact(&encoded).unwrap();
        assert_eq!(decoded, value);
    }
}
