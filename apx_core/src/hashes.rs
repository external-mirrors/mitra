use sha2::{Digest, Sha256};

pub fn sha256(input: &[u8]) -> [u8; 32] {
    Sha256::digest(input).into()
}
