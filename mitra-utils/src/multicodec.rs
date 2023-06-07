/// https://github.com/multiformats/multicodec
/// https://github.com/multiformats/unsigned-varint
use unsigned_varint;

#[derive(thiserror::Error, Debug)]
#[error("multicodec error")]
pub struct MulticodecError;

// Ed25519 public key (ed25519-pub)
const MULTICODEC_ED25519_PUB: u128 = 0xed;
// RSA public key. DER-encoded ASN.1 type RSAPublicKey according to IETF RFC 8017 (PKCS #1)
// (rsa-pub)
const MULTICODEC_RSA_PUB: u128 = 0x1205;

fn encode(code: u128, data: &[u8]) -> Vec<u8> {
    let mut buf: [u8; 19] = Default::default();
    let prefix = unsigned_varint::encode::u128(code, &mut buf).to_vec();
    [prefix, data.to_vec()].concat()
}

fn decode(expected_code: u128, value: &[u8])
    -> Result<Vec<u8>, MulticodecError>
{
    let (code, data) = unsigned_varint::decode::u128(value)
        .map_err(|_| MulticodecError)?;
    if code != expected_code {
        return Err(MulticodecError);
    };
    Ok(data.to_vec())
}

pub fn encode_ed25519_public_key(key: [u8; 32]) -> Vec<u8> {
    encode(MULTICODEC_ED25519_PUB, &key)
}

pub fn decode_ed25519_public_key(value: &[u8])
    -> Result<[u8; 32], MulticodecError>
{
    let data = decode(MULTICODEC_ED25519_PUB, value)?;
    let key: [u8; 32] = data.try_into().map_err(|_| MulticodecError)?;
    Ok(key)
}

pub fn encode_rsa_public_key(key_der: &[u8]) -> Vec<u8> {
    encode(MULTICODEC_RSA_PUB, key_der)
}

pub fn decode_rsa_public_key(value: &[u8]) -> Result<Vec<u8>, MulticodecError> {
    decode(MULTICODEC_RSA_PUB, value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ed25519_pub_encode_decode() {
        let value = [1; 32];
        let encoded = encode_ed25519_public_key(value);
        assert_eq!(encoded.len(), 34);
        let decoded = decode_ed25519_public_key(&encoded).unwrap();
        assert_eq!(decoded, value);
    }

    #[test]
    fn test_rsa_pub_encode_decode() {
        let value = vec![1];
        let encoded = encode_rsa_public_key(&value);
        let decoded = decode_rsa_public_key(&encoded).unwrap();
        assert_eq!(decoded, value);
    }
}
