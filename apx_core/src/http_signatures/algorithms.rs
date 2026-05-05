const CAVAGE_RSA_SHA256: &str = "rsa-sha256";
const CAVAGE_HS2019: &str = "hs2019";

const RFC9421_ED25519: &str = "ed25519";
const RFC9421_RSA_SHA256: &str = "rsa-v1_5-sha256";

/// HTTP signature algorithms
pub enum Algorithm {
    Ed25519,
    RsaSha256,
}

impl Algorithm {
    pub fn as_str_cavage(&self) -> &'static str {
        match self {
            Self::Ed25519 => CAVAGE_HS2019, // Requires SHA-512 ?
            Self::RsaSha256 => CAVAGE_RSA_SHA256,
        }
    }

    pub fn as_str_rfc9421(&self) -> &'static str {
        match self {
            Self::Ed25519 => RFC9421_ED25519,
            Self::RsaSha256 => RFC9421_RSA_SHA256,
        }
    }
}
