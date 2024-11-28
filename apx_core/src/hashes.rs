use sha2::{Digest, Sha256};

use crate::{
    multibase::encode_multibase_base58btc,
    multicodec::{encode_digest, Multicodec},
};

pub fn sha256(input: &[u8]) -> [u8; 32] {
    Sha256::digest(input).into()
}

/// Encodes SHA2-256 digest using multihash and multibase
pub fn sha256_multibase(digest: &[u8]) -> String {
    let digest_sized = encode_digest(digest);
    let digest_multicode = Multicodec::Sha256.encode(&digest_sized);
    encode_multibase_base58btc(&digest_multicode)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multihash_example() {
        // https://github.com/multiformats/multihash?tab=readme-ov-file#example
        let digest = sha256("multihash".as_bytes());
        let output = sha256_multibase(&digest);
        assert_eq!(output, "zQmYtUc4iTCbbfVSDNKvtQqrfyezPPnFvE33wFmutw9PBBk");
    }
}
