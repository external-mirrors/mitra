//! Multihashes
//!
//! <https://github.com/multiformats/multihash>
use thiserror::Error;
use unsigned_varint;

use crate::{
    multibase::{
        decode_multibase_base58btc,
        encode_multibase_base58btc,
        MultibaseError,
    },
    multicodec::{
        Multicodec,
        MulticodecError,
    },
};

/// Errors that may occur when decoding a multihash
#[derive(Debug, Error)]
pub enum MultihashError {
    #[error(transparent)]
    MultibaseError(#[from] MultibaseError),

    #[error(transparent)]
    MulticodecError(#[from] MulticodecError),

    #[error("invalid length")]
    InvalidLength,
}

fn encode_digest(data: &[u8]) -> Vec<u8> {
    let mut buf = unsigned_varint::encode::usize_buffer();
    let size = unsigned_varint::encode::usize(data.len(), &mut buf);
    [size, data].concat()
}

fn decode_digest(value: &[u8]) -> Result<(usize, Vec<u8>), MulticodecError> {
    let (size, data) = unsigned_varint::decode::usize(value)
        .map_err(|_| MulticodecError)?;
    Ok((size, data.to_vec()))
}

/// Encodes SHA2-256 digest using multihash and multibase
pub fn encode_sha256_multihash(digest: &[u8]) -> String {
    let digest_sized = encode_digest(digest);
    let digest_multicode = Multicodec::Sha256.encode(&digest_sized);
    encode_multibase_base58btc(&digest_multicode)
}

/// Decodes SHA2-256 multihash
pub fn decode_sha256_multihash(value: &str) -> Result<[u8; 32], MultihashError> {
    let digest_multicode = decode_multibase_base58btc(value)?;
    let digest_sized = Multicodec::Sha256.decode_exact(&digest_multicode)?;
    let (size, digest) = decode_digest(&digest_sized)?;
    if size != 32 {
        return Err(MultihashError::InvalidLength);
    };
    let digest = digest.try_into()
        .map_err(|_| MultihashError::InvalidLength)?;
    Ok(digest)
}

#[cfg(test)]
mod tests {
    use crate::hashes::sha256;
    use super::*;

    #[test]
    fn test_sha2_256_encode_decode() {
        let digest = [1; 32];
        let sized = encode_digest(&digest);
        assert_eq!(sized.len(), 33);
        let encoded = Multicodec::Sha256.encode(&sized);
        assert_eq!(encoded.len(), 34);
        let decoded_sized = Multicodec::Sha256.decode_exact(&encoded).unwrap();
        let (_, decoded) = decode_digest(&decoded_sized).unwrap();
        assert_eq!(decoded, digest);
    }

    #[test]
    fn multihash_example() {
        // https://github.com/multiformats/multihash?tab=readme-ov-file#example
        let digest = sha256("multihash".as_bytes());
        let output = encode_sha256_multihash(&digest);
        assert_eq!(output, "zQmYtUc4iTCbbfVSDNKvtQqrfyezPPnFvE33wFmutw9PBBk");
    }

    #[test]
    fn test_multihash_encode_decode() {
        let digest = sha256("test".as_bytes());
        let multihash = encode_sha256_multihash(&digest);
        let digest_decoded = decode_sha256_multihash(&multihash).unwrap();
        assert_eq!(digest_decoded, digest);
    }
}
