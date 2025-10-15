//! Hashlinks
//!
//! <https://datatracker.ietf.org/doc/html/draft-sporny-hashlink-07>
use std::fmt;

use regex::Regex;
use thiserror::Error;

use crate::multihash::{
    decode_sha256_multihash,
    encode_sha256_multihash,
    MultihashError,
};

const HASHLINK_RE: &str = r"^hl:(?P<multihash>[a-zA-Z0-9]+)$";

/// Errors that may occur when parsing a hashlink
#[derive(Debug, Error)]
pub enum HashlinkError {
    #[error("invalid URI")]
    InvalidUri,

    #[error(transparent)]
    MultihashError(#[from] MultihashError),
}

/// Hashlink
pub struct Hashlink {
    digest: [u8; 32],
}

impl Hashlink {
    /// Creates a hashlink from a SHA2-256 digest
    pub fn new(digest: [u8; 32]) -> Self {
        Self { digest }
    }

    /// Parses a hashlink
    pub fn parse(value: &str) -> Result<Self, HashlinkError> {
        let hashlink_re = Regex::new(HASHLINK_RE)
            .expect("regexp should be valid");
        let caps = hashlink_re.captures(value)
            .ok_or(HashlinkError::InvalidUri)?;
        let digest = decode_sha256_multihash(&caps["multihash"])?;
        let hashlink = Self { digest };
        Ok(hashlink)
    }

    /// Returns SHA2-256 digest from which this hashlink was created
    pub fn digest(&self) -> [u8; 32] {
        self.digest
    }

    fn multihash(&self) -> String {
        encode_sha256_multihash(self.digest)
    }
}

impl fmt::Display for Hashlink {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "hl:{}", self.multihash())
    }
}

#[cfg(test)]
mod tests {
    use crate::crypto::hashes::sha256;
    use super::*;

    #[test]
    fn test_hashlink_test_value() {
        // https://datatracker.ietf.org/doc/html/draft-sporny-hashlink-07#appendix-B
        let data = "Hello World!";
        let digest = sha256(data.as_bytes());
        let hashlink = Hashlink::new(digest);
        let expected_hashlink = "hl:zQmWvQxTqbG2Z9HPJgG57jjwR154cKhbtJenbyYTWkjgF3e";
        assert_eq!(hashlink.to_string(), expected_hashlink);
    }

    #[test]
    fn test_create_and_parse_hashlink() {
        let digest = sha256(b"test");
        let hashlink = Hashlink::new(digest);
        let hashlink_str = hashlink.to_string();
        let hashlink_parsed = Hashlink::parse(&hashlink_str).unwrap();
        assert_eq!(hashlink_parsed.digest(), digest);
    }
}
