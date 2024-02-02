/// https://www.rfc-editor.org/rfc/rfc3230
use sha2::{Digest, Sha256};

use crate::base64;

pub fn get_sha256_digest(request_body: &[u8]) -> [u8; 32] {
    Sha256::digest(request_body).into()
}

pub(crate) fn get_digest_header(request_body: &[u8]) -> String {
    let digest = get_sha256_digest(request_body);
    let digest_b64 = base64::encode(digest);
    format!("SHA-256={digest_b64}")
}
