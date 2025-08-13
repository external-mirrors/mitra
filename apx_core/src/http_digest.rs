//! HTTP content digest
use regex::Regex;
use sfv::{
    BareItem,
    Item,
    ListEntry,
    Parser,
};

use crate::{
    base64,
    hashes::{sha256, sha512},
};

// https://www.rfc-editor.org/rfc/rfc3230
const DIGEST_RE: &str = r"^(?P<algorithm>[\w-]+)=(?P<digest>[^,]+)(,|$)";

#[derive(Clone, Copy, Debug, PartialEq)]
enum Algorithm {
    Sha256,
    Sha512,
}

impl Algorithm {
    fn parse(algorithm_key: &str) -> Option<Self> {
        // https://www.iana.org/assignments/http-digest-hash-alg/http-digest-hash-alg.xhtml
        let algorithm = match algorithm_key {
            "sha-256" => Self::Sha256,
            "sha-512" => Self::Sha512,
            _ => return None,
        };
        Some(algorithm)
    }

    fn to_str(self) -> &'static str {
        match self {
            Self::Sha256 => "sha-256",
            Self::Sha512 => "sha-512",
        }
    }
}

/// HTTP content digest
#[derive(Clone, Debug, PartialEq)]
pub struct ContentDigest {
    algorithm: Algorithm,
    digest: Vec<u8>,
}

impl ContentDigest {
    /// Creates SHA-256 digest from a request body
    pub fn new(request_body: &[u8]) -> Self {
        let digest = sha256(request_body).to_vec();
        Self { algorithm: Algorithm::Sha256, digest }
    }

    /// Creates SHA-512 digest from a request body
    pub fn new_sha512(request_body: &[u8]) -> Self {
        let digest = sha512(request_body).to_vec();
        Self { algorithm: Algorithm::Sha512, digest }
    }
}

pub(crate) fn create_digest_header(digest: &ContentDigest) -> String {
    let algorithm = digest.algorithm.to_str().to_uppercase();
    let digest_b64 = base64::encode(&digest.digest);
    format!("{algorithm}={digest_b64}")
}

pub(crate) fn parse_digest_header(
    header_value: &str,
) -> Result<ContentDigest, &'static str> {
    let digest_re = Regex::new(DIGEST_RE).expect("regexp should be valid");
    let caps = digest_re.captures(header_value)
        .ok_or("invalid digest header value")?;
    // RFC-3230: digest-algorithm values are case-insensitive
    let algorithm = Algorithm::parse(&caps["algorithm"].to_lowercase())
        .ok_or("unexpected digest algorithm")?;
    let digest_b64 = &caps["digest"];
    let digest = base64::decode(digest_b64)
        .map_err(|_| "invalid digest encoding")?;
    Ok(ContentDigest { algorithm, digest })
}

/// <https://datatracker.ietf.org/doc/html/rfc9530#section-2>
pub(crate) fn parse_content_digest_header(
    header_value: &str,
) -> Result<ContentDigest, &'static str> {
    let dict = Parser::parse_dictionary(header_value.as_bytes())
        .map_err(|_| "invalid content-digest header")?;
    let (label, list_item) = dict.first()
        .ok_or("invalid content-digest header")?;
    let algorithm = Algorithm::parse(label)
        .ok_or("unexpected digest algorithm")?;
    let digest = match list_item {
        ListEntry::Item(Item { bare_item: BareItem::ByteSeq(value), .. }) => {
            value.clone()
        },
        _ => return Err("invalid digest encoding"),
    };
    Ok(ContentDigest { algorithm, digest })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_and_parse_digest_header() {
        let request_body = "test*123";
        let digest = ContentDigest::new(request_body.as_bytes());
        let header_value = create_digest_header(&digest);
        let parsed = parse_digest_header(&header_value).unwrap();
        assert_eq!(parsed, digest);
    }

    #[test]
    fn test_create_and_parse_digest_header_sha512() {
        let request_body = "test*123";
        let digest_sha512 = ContentDigest::new_sha512(request_body.as_bytes());
        let header_value = create_digest_header(&digest_sha512);
        let parsed = parse_digest_header(&header_value).unwrap();
        assert_eq!(parsed, digest_sha512);
    }

    #[test]
    fn test_parse_digest_header_multiple_digests() {
        let request_body = "test*123";
        let digest = ContentDigest::new(request_body.as_bytes());
        let digest_b64 = base64::encode(&digest.digest);
        let header_value = format!("sha-256={digest_b64},unixsum=30637");
        let parsed = parse_digest_header(&header_value).unwrap();
        assert_eq!(parsed, digest);
    }

    #[test]
    fn test_parse_content_digest_header() {
        // https://datatracker.ietf.org/doc/html/rfc9530#name-sample-digest-values
        let request_body = r#"{"hello": "world"}"#;
        let expected_digest = ContentDigest::new_sha512(request_body.as_bytes());
        let header_value = "sha-512=:WZDPaVn/7XgHaAy8pmojAkGWoRx2UFChF41A2svX+TaPm+AbwAgBWnrIiYllu7BNNyealdVLvRwEmTHWXvJwew==:";
        let digest = parse_content_digest_header(header_value).unwrap();
        assert_eq!(digest, expected_digest);
    }
}
