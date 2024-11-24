use apx_core::hashes::sha256;

pub fn hash_oauth_token(token: &str) -> [u8; 32] {
    sha256(token.as_bytes())
}
