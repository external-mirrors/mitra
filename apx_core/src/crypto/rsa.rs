//! RSA utilities
use rsa::{
    pkcs1::{
        DecodeRsaPrivateKey,
        DecodeRsaPublicKey,
        EncodeRsaPrivateKey,
        EncodeRsaPublicKey,
    },
    pkcs8::{
        DecodePrivateKey,
        DecodePublicKey,
        EncodePrivateKey,
        EncodePublicKey,
        LineEnding,
    },
    pkcs1v15::{Signature, SigningKey, VerifyingKey},
    signature::{
        SignatureEncoding,
        Signer,
        Verifier,
    },
};
use sha2::Sha256;

use crate::{
    multibase::{decode_multibase_base58btc, encode_multibase_base58btc},
    multicodec::Multicodec,
};

pub use rsa::{RsaPrivateKey as RsaSecretKey, RsaPublicKey};
pub type RsaError = rsa::errors::Error;

pub fn generate_rsa_key() -> Result<RsaSecretKey, RsaError> {
    let mut rng = rand::rngs::OsRng;
    let bits = 2048;
    RsaSecretKey::new(&mut rng, bits)
}

#[cfg(any(test, feature = "test-utils"))]
pub fn generate_weak_rsa_key() -> Result<RsaSecretKey, RsaError> {
    use rand::SeedableRng;
    let mut rng = rand::rngs::StdRng::seed_from_u64(0);
    let bits = 512;
    RsaSecretKey::new(&mut rng, bits)
}

#[derive(thiserror::Error, Debug)]
pub enum RsaSerializationError {
    #[error(transparent)]
    Pkcs1Error(#[from] rsa::pkcs1::Error),

    #[error(transparent)]
    Pkcs8Error(#[from] rsa::pkcs8::Error),

    #[error(transparent)]
    PemError(#[from] pem::PemError),

    #[error("multikey error")]
    MultikeyError,
}

pub fn rsa_secret_key_to_pkcs1_der(
    secret_key: &RsaSecretKey,
) -> Result<Vec<u8>, RsaSerializationError> {
    let bytes = secret_key.to_pkcs1_der()?.as_bytes().to_vec();
    Ok(bytes)
}

pub fn rsa_secret_key_from_pkcs1_der(
    bytes: &[u8],
) -> Result<RsaSecretKey, RsaSerializationError> {
    let secret_key = RsaSecretKey::from_pkcs1_der(bytes)?;
    Ok(secret_key)
}

pub fn rsa_public_key_to_pkcs1_der(
    public_key: &RsaPublicKey,
) -> Result<Vec<u8>, RsaSerializationError> {
    let bytes = public_key.to_pkcs1_der()?.to_vec();
    Ok(bytes)
}

pub fn rsa_public_key_from_pkcs1_der(
    bytes: &[u8],
) -> Result<RsaPublicKey, RsaSerializationError> {
    let public_key = RsaPublicKey::from_pkcs1_der(bytes)?;
    Ok(public_key)
}

pub fn rsa_secret_key_to_pkcs8_pem(
    secret_key: &RsaSecretKey,
) -> Result<String, RsaSerializationError> {
    let secret_key_pem = secret_key.to_pkcs8_pem(LineEnding::LF)
        .map(|val| val.to_string())?;
    Ok(secret_key_pem)
}

pub fn rsa_secret_key_from_pkcs8_pem(
    secret_key_pem: &str,
) -> Result<RsaSecretKey, RsaSerializationError> {
    let secret_key = RsaSecretKey::from_pkcs8_pem(secret_key_pem)?;
    Ok(secret_key)
}

pub fn rsa_secret_key_to_multikey(
    secret_key: &RsaSecretKey,
) -> Result<String, RsaSerializationError> {
    let secret_key_der = rsa_secret_key_to_pkcs1_der(secret_key)?;
    let secret_key_multicode = Multicodec::RsaPriv.encode(&secret_key_der);
    let secret_key_multibase = encode_multibase_base58btc(&secret_key_multicode);
    Ok(secret_key_multibase)
}

pub fn rsa_secret_key_from_multikey(
    secret_key_multibase: &str,
) -> Result<RsaSecretKey, RsaSerializationError> {
    let secret_key_multicode = decode_multibase_base58btc(secret_key_multibase)
        .map_err(|_| RsaSerializationError::MultikeyError)?;
    let secret_key_der = Multicodec::RsaPriv.decode_exact(&secret_key_multicode)
        .map_err(|_| RsaSerializationError::MultikeyError)?;
    let secret_key = rsa_secret_key_from_pkcs1_der(&secret_key_der)?;
    Ok(secret_key)
}

pub fn rsa_public_key_to_pkcs8_pem(
    public_key: &RsaPublicKey,
) -> Result<String, RsaSerializationError> {
    let public_key_pem = public_key.to_public_key_pem(LineEnding::LF)
        .map_err(rsa::pkcs8::Error::from)?;
    Ok(public_key_pem)
}

pub fn deserialize_rsa_public_key(
    public_key_pem: &str,
) -> Result<RsaPublicKey, RsaSerializationError> {
    if public_key_pem.contains("BEGIN RSA PUBLIC KEY") {
        let public_key = RsaPublicKey::from_pkcs1_pem(public_key_pem.trim())?;
        return Ok(public_key);
    };
    // rsa package can't decode PEM string with non-standard wrap width,
    // so the input should be normalized first
    let parsed_pem = pem::parse(public_key_pem.trim().as_bytes())?;
    let normalized_pem = pem::encode(&parsed_pem);
    let public_key = RsaPublicKey::from_public_key_pem(&normalized_pem)
        .map_err(rsa::pkcs8::Error::from)?;
    Ok(public_key)
}

pub fn rsa_public_key_to_multikey(
    public_key: &RsaPublicKey,
) -> Result<String, RsaSerializationError> {
    let public_key_der = rsa_public_key_to_pkcs1_der(public_key)?;
    let public_key_multicode = Multicodec::RsaPub.encode(&public_key_der);
    let public_key_multibase = encode_multibase_base58btc(&public_key_multicode);
    Ok(public_key_multibase)
}

pub fn rsa_public_key_from_multikey(
    multikey: &str,
) -> Result<RsaPublicKey, RsaSerializationError> {
    let public_key_multicode = decode_multibase_base58btc(multikey)
        .map_err(|_| RsaSerializationError::MultikeyError)?;
    let public_key_der =
        Multicodec::RsaPub.decode_exact(&public_key_multicode)
            .map_err(|_| RsaSerializationError::MultikeyError)?;
    let public_key = rsa_public_key_from_pkcs1_der(&public_key_der)?;
    Ok(public_key)
}

/// RSASSA-PKCS1-v1_5 signature
pub fn create_rsa_sha256_signature(
    secret_key: &RsaSecretKey,
    message: &[u8],
) -> Result<Vec<u8>, RsaError> {
    let signing_key = SigningKey::<Sha256>::new(secret_key.clone());
    let signature = signing_key.sign(message);
    Ok(signature.to_vec())
}

pub fn verify_rsa_sha256_signature(
    public_key: &RsaPublicKey,
    message: &[u8],
    signature: &[u8],
) -> Result<(), RsaError> {
    let verifying_key = VerifyingKey::<Sha256>::new(public_key.clone());
    let signature = match Signature::try_from(signature) {
        Ok(signature) => signature,
        // TODO: the type of error is k256::ecdsa::Error
        Err(_) => return Err(RsaError::Verification),
    };
    // TODO: the type of error is k256::ecdsa::Error
    verifying_key.verify(message, &signature)
        .map_err(|_| RsaError::Verification)
}

#[cfg(test)]
mod tests {
    use crate::base64;
    use super::*;

    #[test]
    fn test_secret_key_pkcs1_der_encode_decode() {
        let secret_key = generate_weak_rsa_key().unwrap();
        let encoded = rsa_secret_key_to_pkcs1_der(&secret_key).unwrap();
        let decoded = rsa_secret_key_from_pkcs1_der(&encoded).unwrap();
        assert_eq!(decoded, secret_key);
    }

    #[test]
    fn test_public_key_pkcs1_der_encode_decode() {
        let secret_key = generate_weak_rsa_key().unwrap();
        let public_key = RsaPublicKey::from(secret_key);
        let encoded = rsa_public_key_to_pkcs1_der(&public_key).unwrap();
        let decoded = rsa_public_key_from_pkcs1_der(&encoded).unwrap();
        assert_eq!(decoded, public_key);
    }

    #[test]
    fn test_secret_key_pkcs8_pem_encode_decode() {
        let secret_key = generate_weak_rsa_key().unwrap();
        let encoded = rsa_secret_key_to_pkcs8_pem(&secret_key).unwrap();
        let decoded = rsa_secret_key_from_pkcs8_pem(&encoded).unwrap();
        assert_eq!(decoded, secret_key);
    }

    #[test]
    fn test_secret_key_multikey_encode_decode() {
        let secret_key = generate_weak_rsa_key().unwrap();
        let encoded = rsa_secret_key_to_multikey(&secret_key).unwrap();
        let decoded = rsa_secret_key_from_multikey(&encoded).unwrap();
        assert_eq!(decoded, secret_key);
    }

    #[test]
    fn test_deserialize_rsa_public_key_nowrap() {
        let public_key_pem = "-----BEGIN PUBLIC KEY-----\nMIGfMA0GCSqGSIb3DQEBAQUAA4GNADCBiQKBgQC8ehqQ7n6+pw19U8q2UtxE/9017STW3yRnnqV5nVk8LJ00ba+berqwekxDW+nw77GAu3TJ+hYeeSerUNPup7y3yO3V
YsFtrgWDQ/s8k86sNBU+Ce2GOL7seh46kyAWgJeohh4Rcrr23rftHbvxOcRM8VzYuCeb1DgVhPGtA0xULwIDAQAB\n-----END PUBLIC KEY-----";
        let result = deserialize_rsa_public_key(&public_key_pem);
        assert_eq!(result.is_ok(), true);
    }

    #[test]
    fn test_deserialize_rsa_public_key_pkcs1() {
        let public_key_pem = "-----BEGIN RSA PUBLIC KEY-----\nMIIBCgKCAQEA2vzT/2X+LqqoLFLJAZDMGRoAaXEyw9NBCGpu6wTqczs2KEGHdbQe\nIEGKr/+tP6ENOtwe74I2cCsKOPCzUMWTqu2JRd7zfDXUmQnzIZ9wp3AZQ6YFZspj\nxNAzC3dIR6dQr0feebqZZ3n/t7n1ch04Onc2SINyS7MLQHxNi9HTkH9OXZSHDazP\nT8T90Zr2oxo16nVs8rxTVxtE/6bZai90xrSEOfvJfE/0fwb5BK9Fw3J4yv5h+4ck\nrUoSFGEBrTRGgwCrp3UDt/K6Lp4loVC9jzyRMJ5bo5n1rZNgjNCqEqBrJFu6AWSC\nWW/eqkdipgI2IlRezppu0balvEwEluPhNwIDAQAB\n-----END RSA PUBLIC KEY-----\n\n";
        let result = deserialize_rsa_public_key(&public_key_pem);
        assert_eq!(result.is_ok(), true);
    }

    #[test]
    fn test_public_key_serialization_deserialization() {
        let secret_key = generate_weak_rsa_key().unwrap();
        let public_key = RsaPublicKey::from(&secret_key);
        let public_key_pem = rsa_public_key_to_pkcs8_pem(&public_key).unwrap();
        let public_key = deserialize_rsa_public_key(&public_key_pem).unwrap();
        assert_eq!(public_key, RsaPublicKey::from(&secret_key));
    }

    #[test]
    fn test_public_key_multikey_encode_decode() {
        let secret_key = generate_weak_rsa_key().unwrap();
        let public_key = RsaPublicKey::from(&secret_key);
        let encoded = rsa_public_key_to_multikey(&public_key).unwrap();
        let decoded = rsa_public_key_from_multikey(&encoded).unwrap();
        assert_eq!(decoded, public_key);
    }

    #[test]
    fn test_verify_rsa_signature() {
        let public_key_pem = "-----BEGIN PUBLIC KEY-----
MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEA1wOPLWAp6nNT5CwttzFP
kWKm+U+weptmldt+EC0JcSuc8sWwki4BU5k/4zunCCc0jxN4ZrfAv3LmYDAx4nbC
Z+7ndKFrDjLtcHMsBBmb+/YYH4lXBXmauMLYGVMZg/8/xPh/euzu+u7wBtFLXU1D
j9PKrqccKZ1I2ENQxTCPMdCI4BYR9niZcKjqG4lVKIbb4VCzIITlVJL7KNt2ZyYX
IjxLKfnZVfCkQ9t5EWkoBME8Gf8hKltxcA5jvEbgHxwmFKgIeSZXg3gQncQL1/qZ
8AVcpaMTTqahxPCFRExlRU0y8ppGcqymyMH/P6jHclRZDqxtwT/S3nFPbwuBAx4O
NwIDAQAB
-----END PUBLIC KEY-----";
        let message = b"test";
        let signature = "NFiY1Vx+jZizdiLvS4JAoxcsCI2+SjwWPdWsj8ICqRuMcMg0Gu7/qPu2n/B8sUjXycZH0sUcATIbHaf7AtPTNEU/FDFP+1wR5K4fCEt6QpaV4uGR8KBYTJUV2vE6nnx2Hkr/bAhK8JM3f4OQATqxDc7Ozmosd48sx3alxOOGgZnQD3kCKVhaSJH/ZkYAcPmY7ksSbm9iFX09D2ytEp+FDAD3pzgiNq/MlmozAmSdX9/cS2IFbKAjiJ3wq1T4NqApTZ0Rd8HYuBveMnW3GVeyPalao7uIaYyJumqaf9cBg9l9EkwGwJZ5gsoAV5OHgMTU5bMGF1ShR5xWCnG8fq1ylg==";

        let public_key = deserialize_rsa_public_key(public_key_pem).unwrap();
        let signature_bytes = base64::decode(signature).unwrap();
        let is_valid = verify_rsa_sha256_signature(
            &public_key,
            message,
            &signature_bytes,
        ).is_ok();
        assert_eq!(is_valid, true);
    }

    #[test]
    fn test_create_and_verify_rsa_signature() {
        let secret_key = generate_weak_rsa_key().unwrap();
        let message = b"test";
        let signature = create_rsa_sha256_signature(
            &secret_key,
            message,
        ).unwrap();
        let public_key = RsaPublicKey::from(&secret_key);

        let is_valid = verify_rsa_sha256_signature(
            &public_key,
            message,
            &signature,
        ).is_ok();
        assert_eq!(is_valid, true);
    }
}
