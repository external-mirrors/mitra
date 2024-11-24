//! HTTP content digest
use regex::Regex;

use crate::{
    base64,
    hashes::sha256,
};

// https://www.rfc-editor.org/rfc/rfc3230
const DIGEST_RE: &str = r"^(?P<algorithm>[\w-]+)=(?P<digest>[^,]+)(,|$)";

/// HTTP content digest
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContentDigest([u8; 32]);

impl ContentDigest {
    /// Creates SHA-256 digest from a request body
    pub fn new(request_body: &[u8]) -> Self {
        let digest = sha256(request_body);
        Self(digest)
    }
}

pub(crate) fn get_digest_header(request_body: &[u8]) -> String {
    let digest = sha256(request_body);
    let digest_b64 = base64::encode(digest);
    format!("SHA-256={digest_b64}")
}

pub(crate) fn parse_digest_header(
    header_value: &str,
) -> Result<ContentDigest, &'static str> {
    let digest_re = Regex::new(DIGEST_RE).expect("regexp should be valid");
    let caps = digest_re.captures(header_value)
        .ok_or("invalid digest header value")?;
    // RFC-3230: digest-algorithm values are case-insensitive
    let algorithm = caps["algorithm"].to_uppercase();
    if algorithm != "SHA-256" {
        return Err("unexpected digest algorithm");
    };
    let digest_b64 = &caps["digest"];
    let digest = base64::decode(digest_b64)
        .map_err(|_| "invalid digest encoding")?
        .try_into()
        .map_err(|_| "invalid digest length")?;
    Ok(ContentDigest(digest))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_digest_header() {
        let request_body = "test*123";
        let digest = ContentDigest::new(request_body.as_bytes());
        let header_value = get_digest_header(request_body.as_bytes());
        let parsed = parse_digest_header(&header_value).unwrap();
        assert_eq!(parsed, digest);
    }

    #[test]
    fn test_parse_digest_header_multiple_digests() {
        let request_body = "test*123";
        let digest = ContentDigest::new(request_body.as_bytes());
        let digest_b64 = base64::encode(digest.0);
        let header_value = format!("sha-256={digest_b64},unixsum=30637");
        let parsed = parse_digest_header(&header_value).unwrap();
        assert_eq!(parsed, digest);
    }
}
