//! JSON Canonicalization Scheme (JCS)
//!
//! <https://www.rfc-editor.org/rfc/rfc8785>
use serde::Serialize;

#[derive(thiserror::Error, Debug)]
#[error("canonicalization error")]
pub struct CanonicalizationError;

/// Performs JCS canonicalization
pub fn canonicalize_object(
    object: &impl Serialize,
) -> Result<String, CanonicalizationError> {
    let jcs_bytes = serde_json_canonicalizer::to_vec(object)
        .map_err(|_| CanonicalizationError)?;
    let jcs_string = String::from_utf8(jcs_bytes)
        .map_err(|_| CanonicalizationError)?;
    Ok(jcs_string)
}
