/// https://github.com/multiformats/multicodec
/// https://github.com/multiformats/unsigned-varint
use unsigned_varint;

#[derive(thiserror::Error, Debug)]
#[error("multicodec error")]
pub struct MulticodecError;

// Ed25519 public key (ed25519-pub)
const MULTICODEC_ED25519_PUB: u128 = 0xed;
// Ed25519 private key (ed25519-priv)
const MULTICODEC_ED25519_PRIV: u128 = 0x1300;
// RSA public key. DER-encoded ASN.1 type RSAPublicKey according to IETF RFC 8017 (PKCS #1)
// (rsa-pub)
const MULTICODEC_RSA_PUB: u128 = 0x1205;
// RSA private key (rsa-priv)
const MULTICODEC_RSA_PRIV: u128 = 0x1305;

fn encode(code: u128, data: &[u8]) -> Vec<u8> {
    let mut buf: [u8; 19] = Default::default();
    let code = unsigned_varint::encode::u128(code, &mut buf).to_vec();
    [code, data.to_vec()].concat()
}

fn decode(value: &[u8]) -> Result<(u128, Vec<u8>), MulticodecError> {
    let (code, data) = unsigned_varint::decode::u128(value)
        .map_err(|_| MulticodecError)?;
    Ok((code, data.to_vec()))
}

pub enum Multicodec {
    Ed25519Pub,
    Ed25519Priv,
    RsaPub,
    RsaPriv,
}

impl Multicodec {
    fn from_code(code: u128) -> Result<Self, MulticodecError> {
        let codec = match code {
            MULTICODEC_ED25519_PUB => Self::Ed25519Pub,
            MULTICODEC_ED25519_PRIV => Self::Ed25519Priv,
            MULTICODEC_RSA_PUB => Self::RsaPub,
            MULTICODEC_RSA_PRIV => Self::RsaPriv,
            _ => return Err(MulticodecError),
        };
        Ok(codec)
    }

    fn code(&self) -> u128 {
        match self {
            Self::Ed25519Pub => MULTICODEC_ED25519_PUB,
            Self::Ed25519Priv => MULTICODEC_ED25519_PRIV,
            Self::RsaPub => MULTICODEC_RSA_PUB,
            Self::RsaPriv => MULTICODEC_RSA_PRIV,
        }
    }

    pub fn encode(&self, data: &[u8]) -> Vec<u8> {
        encode(self.code(), data)
    }

    fn decode_exact(&self, value: &[u8]) -> Result<Vec<u8>, MulticodecError> {
        let (code, data) = decode(value)?;
        if code != self.code() {
            return Err(MulticodecError);
        };
        Ok(data)
    }

    pub fn decode(value: &[u8]) -> Result<(Self, Vec<u8>), MulticodecError> {
        let (code, data) = decode(value)?;
        let codec = Self::from_code(code)?;
        Ok((codec, data))
    }
}

pub fn encode_ed25519_public_key(key: [u8; 32]) -> Vec<u8> {
    Multicodec::Ed25519Pub.encode(&key)
}

pub fn decode_ed25519_public_key(value: &[u8])
    -> Result<[u8; 32], MulticodecError>
{
    let data = Multicodec::Ed25519Pub.decode_exact(value)?;
    let key: [u8; 32] = data.try_into().map_err(|_| MulticodecError)?;
    Ok(key)
}

pub fn encode_ed25519_private_key(key: [u8; 32]) -> Vec<u8> {
    Multicodec::Ed25519Priv.encode(&key)
}

pub fn decode_ed25519_private_key(value: &[u8])
    -> Result<[u8; 32], MulticodecError>
{
    let data = Multicodec::Ed25519Priv.decode_exact(value)?;
    let key: [u8; 32] = data.try_into().map_err(|_| MulticodecError)?;
    Ok(key)
}

pub fn encode_rsa_public_key(key_der: &[u8]) -> Vec<u8> {
    Multicodec::RsaPub.encode(key_der)
}

pub fn decode_rsa_public_key(value: &[u8]) -> Result<Vec<u8>, MulticodecError> {
    Multicodec::RsaPub.decode_exact(value)
}

pub fn encode_rsa_private_key(key: &[u8]) -> Vec<u8> {
    Multicodec::RsaPriv.encode(key)
}

pub fn decode_rsa_private_key(value: &[u8]) -> Result<Vec<u8>, MulticodecError> {
    Multicodec::RsaPriv.decode_exact(value)
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
    fn test_ed25519_priv_encode_decode() {
        let value = [2; 32];
        let encoded = encode_ed25519_private_key(value);
        let decoded = decode_ed25519_private_key(&encoded).unwrap();
        assert_eq!(decoded, value);
    }

    #[test]
    fn test_rsa_pub_encode_decode() {
        let value = vec![1];
        let encoded = encode_rsa_public_key(&value);
        let decoded = decode_rsa_public_key(&encoded).unwrap();
        assert_eq!(decoded, value);
    }

    #[test]
    fn test_rsa_priv_encode_decode() {
        let value = vec![1];
        let encoded = encode_rsa_private_key(&value);
        let decoded = decode_rsa_private_key(&encoded).unwrap();
        assert_eq!(decoded, value);
    }

    #[test]
    fn test_decode() {
        let value = [1; 32];
        let encoded = encode_ed25519_public_key(value);
        let (codec, decoded) = Multicodec::decode(&encoded).unwrap();
        assert!(matches!(codec, Multicodec::Ed25519Pub));
        assert_eq!(decoded, value);
    }
}
