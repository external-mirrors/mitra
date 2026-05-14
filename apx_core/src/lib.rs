//! # APx core primitives
//!
//! - URIs
//! - DIDs
//! - Keys and signatures (Ed25519 and RSA)
//! - HTTP signatures (Draft-Cavage and RFC-9421)
//! - Data integrity proofs (FEP-8b32)
//!
//! ## Examples
//!
//! Create an HTTP signature:
//!
//! ```rust
//! use apx_core::{
//!     crypto::rsa::generate_rsa_key,
//!     http_signatures::create::{create_http_signature_cavage, HttpSigner},
//!     http_types::Method,
//! };
//! let request_uri = "https://verifier.example/inbox";
//! let request_body = r#"{"type":"Note"}"#;
//! let signer_key = generate_rsa_key().expect("should generate secret key");
//! let signer_key_id = "https://signer.example/actor#main-key";
//! let signer = HttpSigner::new_rsa(signer_key, signer_key_id.to_owned());
//! let signed_headers = create_http_signature_cavage(
//!     Method::POST,
//!     request_uri,
//!     Some(request_body.as_bytes()),
//!     &signer,
//! ).expect("should create signature");
//! assert_eq!(signed_headers.host, "verifier.example");
//! ```

pub mod base64;
pub mod crypto;
pub mod did;
pub mod did_key;
pub mod did_url;
pub mod hashlink;
pub mod http_digest;
pub mod http_signatures;
pub mod http_types;
pub mod http_utils;
pub mod jcs;
pub mod json_signatures;
pub mod media_type;
pub mod multibase;
pub mod multicodec;
pub mod multihash;
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
