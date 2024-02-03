/// https://www.rfc-editor.org/rfc/rfc3230
use regex::Regex;
use sha2::{Digest, Sha256};

use crate::base64;

const DIGEST_RE: &str = r"^SHA-256=(?P<digest>.+)$";

pub fn get_sha256_digest(request_body: &[u8]) -> [u8; 32] {
    Sha256::digest(request_body).into()
}

pub(crate) fn get_digest_header(request_body: &[u8]) -> String {
    let digest = get_sha256_digest(request_body);
    let digest_b64 = base64::encode(digest);
    format!("SHA-256={digest_b64}")
}

pub(crate) fn parse_digest_header(
    header_value: &str,
) -> Result<[u8; 32], &'static str>  {
    let digest_re = Regex::new(DIGEST_RE).expect("regexp should be valid");
    let caps = digest_re.captures(header_value)
        .ok_or("invalid digest header value")?;
    let digest_b64 = &caps["digest"];
    let digest = base64::decode(digest_b64)
        .map_err(|_| "invalid digest encoding")?
        .try_into()
        .map_err(|_| "invalid digest length")?;
    Ok(digest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_digest_header() {
        let request_body = "test*123";
        let digest = get_sha256_digest(request_body.as_bytes());
        let header_value = get_digest_header(request_body.as_bytes());
        let parsed = parse_digest_header(&header_value).unwrap();
        assert_eq!(parsed, digest);
    }
}
