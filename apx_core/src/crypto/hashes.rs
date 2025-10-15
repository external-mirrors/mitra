use sha2::{Digest, Sha256, Sha512};

/// Computes SHA-256 digest of the given value
pub fn sha256(input: &[u8]) -> [u8; 32] {
    Sha256::digest(input).into()
}

/// Computes SHA-512 digest of the given value
pub(crate) fn sha512(input: &[u8]) -> [u8; 64] {
    Sha512::digest(input).into()
}
