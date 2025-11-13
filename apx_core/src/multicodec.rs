//! Multicodecs
//!
//! <https://github.com/multiformats/multicodec>  
//! <https://github.com/multiformats/unsigned-varint>
use unsigned_varint;

#[derive(thiserror::Error, Debug)]
#[error("multicodec error")]
pub struct MulticodecError;

// SHA2 256 bit (sha2-256)
const MULTICODEC_SHA2_256: u128 = 0x12;
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
    let mut buf = unsigned_varint::encode::u128_buffer();
    let code = unsigned_varint::encode::u128(code, &mut buf);
    [code, data].concat()
}

fn decode(value: &[u8]) -> Result<(u128, Vec<u8>), MulticodecError> {
    let (code, data) = unsigned_varint::decode::u128(value)
        .map_err(|_| MulticodecError)?;
    Ok((code, data.to_vec()))
}

#[derive(Clone, Debug, PartialEq)]
pub enum Multicodec {
    Sha256,
    Ed25519Pub,
    Ed25519Priv,
    RsaPub,
    RsaPriv,
}

impl Multicodec {
    fn from_code(code: u128) -> Result<Self, MulticodecError> {
        let codec = match code {
            MULTICODEC_SHA2_256 => Self::Sha256,
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
            Self::Sha256 => MULTICODEC_SHA2_256,
            Self::Ed25519Pub => MULTICODEC_ED25519_PUB,
            Self::Ed25519Priv => MULTICODEC_ED25519_PRIV,
            Self::RsaPub => MULTICODEC_RSA_PUB,
            Self::RsaPriv => MULTICODEC_RSA_PRIV,
        }
    }

    pub fn encode(&self, data: &[u8]) -> Vec<u8> {
        encode(self.code(), data)
    }

    pub fn decode_exact(&self, value: &[u8]) -> Result<Vec<u8>, MulticodecError> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ed25519_pub_encode_decode() {
        let value = [1; 32];
        let encoded = Multicodec::Ed25519Pub.encode(&value);
        assert_eq!(encoded.len(), 34);
        let decoded = Multicodec::Ed25519Pub.decode_exact(&encoded).unwrap();
        assert_eq!(decoded, value);
    }

    #[test]
    fn test_ed25519_priv_encode_decode() {
        let value = [2; 32];
        let encoded = Multicodec::Ed25519Priv.encode(&value);
        let decoded = Multicodec::Ed25519Priv.decode_exact(&encoded).unwrap();
        assert_eq!(decoded, value);
    }

    #[test]
    fn test_rsa_pub_encode_decode() {
        let value = vec![1];
        let encoded = Multicodec::RsaPub.encode(&value);
        let decoded = Multicodec::RsaPub.decode_exact(&encoded).unwrap();
        assert_eq!(decoded, value);
    }

    #[test]
    fn test_rsa_priv_encode_decode() {
        let value = vec![1];
        let encoded = Multicodec::RsaPriv.encode(&value);
        let decoded = Multicodec::RsaPriv.decode_exact(&encoded).unwrap();
        assert_eq!(decoded, value);
    }

    #[test]
    fn test_decode() {
        let value = [1; 32];
        let encoded = Multicodec::Ed25519Pub.encode(&value);
        let (codec, decoded) = Multicodec::decode(&encoded).unwrap();
        assert_eq!(codec, Multicodec::Ed25519Pub);
        assert_eq!(decoded, value);
    }
}
