/// Proof types
use std::str::FromStr;

// Identity proof, version 00
pub const PROOF_TYPE_ID_EIP191: &str = "ethereum-eip191-00";

// Identity proof, version 2022A
pub const PROOF_TYPE_ID_MINISIGN: &str = "MitraMinisignSignature2022A";

// https://w3c.github.io/vc-data-integrity/#dataintegrityproof
pub const DATA_INTEGRITY_PROOF: &str = "DataIntegrityProof";

// Similar to https://identity.foundation/JcsEd25519Signature2020/
// - Canonicalization algorithm: JCS
// - Digest algorithm: SHA-256
// - Signature algorithm: RSASSA-PKCS1-v1_5
pub const PROOF_TYPE_JCS_RSA: &str = "MitraJcsRsaSignature2022";
pub const PROOF_TYPE_JCS_RSA_LEGACY: &str = "JcsRsaSignature2022";
pub const CRYPTOSUITE_JCS_RSA: &str = "mitra-jcs-rsa-2022";

// Similar to EthereumPersonalSignature2021 but with JCS
pub const PROOF_TYPE_JCS_EIP191: &str = "MitraJcsEip191Signature2022";
pub const PROOF_TYPE_JCS_EIP191_LEGACY: &str ="JcsEip191Signature2022";

// Similar to Ed25519Signature2020
// https://w3c-ccg.github.io/di-eddsa-2020/#ed25519signature2020
// - Canonicalization algorithm: JCS
// - Digest algorithm: BLAKE2b-512
// - Signature algorithm: EdDSA
pub const PROOF_TYPE_JCS_BLAKE2_ED25519: &str = "MitraJcsEd25519Signature2022";

// https://w3c.github.io/vc-di-eddsa/#jcs-eddsa-2022
pub const CRYPTOSUITE_JCS_EDDSA_LEGACY: &str = "jcs-eddsa-2022";
pub const CRYPTOSUITE_JCS_EDDSA: &str = "eddsa-jcs-2022";

#[derive(Debug, PartialEq)]
pub enum ProofType {
    JcsEip191Signature,
    JcsBlake2Ed25519Signature,
    JcsRsaSignature,
    JcsEddsaSignature,
}

#[derive(thiserror::Error, Debug)]
#[error("unsupported proof type")]
pub struct UnsupportedProofType;

impl FromStr for ProofType {
    type Err = UnsupportedProofType;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let proof_type = match value {
            PROOF_TYPE_JCS_EIP191 => Self::JcsEip191Signature,
            PROOF_TYPE_JCS_EIP191_LEGACY => Self::JcsEip191Signature,
            PROOF_TYPE_JCS_BLAKE2_ED25519 => Self::JcsBlake2Ed25519Signature,
            PROOF_TYPE_JCS_RSA => Self::JcsRsaSignature,
            PROOF_TYPE_JCS_RSA_LEGACY => Self::JcsRsaSignature,
            _ => return Err(UnsupportedProofType),
        };
        Ok(proof_type)
    }
}

impl ProofType {
    pub fn from_cryptosuite(value: &str) -> Result<Self, UnsupportedProofType> {
        let proof_type = match value {
            CRYPTOSUITE_JCS_RSA => Self::JcsRsaSignature,
            CRYPTOSUITE_JCS_EDDSA_LEGACY => Self::JcsEddsaSignature,
            CRYPTOSUITE_JCS_EDDSA => Self::JcsEddsaSignature,
            _ => return Err(UnsupportedProofType),
        };
        Ok(proof_type)
    }
}
