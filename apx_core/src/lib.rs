//! # APx core primitives
//!
//! - URIs
//! - DIDs
//! - Keys and signatures (Ed25519 and RSA)
//! - HTTP signatures
//! - Data integrity proofs (FEP-8b32)

pub mod ap_url;
pub mod base64;
pub mod crypto;
pub mod crypto_ecdsa;
pub mod crypto_eddsa;
pub mod crypto_rsa;
pub mod did;
pub mod did_key;
pub mod did_url;
pub mod hashes;
pub mod http_digest;
pub mod http_signatures;
pub mod http_types;
pub mod http_url;
pub mod http_url_whatwg;
pub mod http_utils;
pub mod jcs;
pub mod json_signatures;
pub mod media_type;
pub mod multibase;
pub mod multicodec;
pub mod url;

#[cfg(feature = "caip")]
pub mod caip10;
#[cfg(feature = "caip")]
pub mod caip19;
#[cfg(feature = "caip")]
pub mod caip2;

#[cfg(feature = "did-pkh")]
pub mod did_pkh;

#[cfg(feature = "eip191")]
pub mod eip191;

#[cfg(feature = "minisign")]
pub mod minisign;
