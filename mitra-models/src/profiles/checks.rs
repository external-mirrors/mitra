use std::collections::HashSet;

use crate::database::DatabaseTypeError;

use super::types::{DbActorKey, IdentityProof};

pub fn check_public_keys(
    public_keys: &[DbActorKey],
    is_remote: bool,
) -> Result<(), DatabaseTypeError> {
    if is_remote {
        if public_keys.is_empty() {
            // Remote actor must have at least one public key
            return Err(DatabaseTypeError);
        };
        let mut ids = HashSet::new();
        // HashSet::insert returns true if the value is unique
        if !public_keys.iter().map(|key| &key.id).all(|id| ids.insert(id)) {
            // Public key IDs must be unique
            return Err(DatabaseTypeError);
        };
    };
    if !is_remote && !public_keys.is_empty() {
        // Local actor must have no public keys"
        return Err(DatabaseTypeError);
    };
    Ok(())
}

pub fn check_identity_proofs(
    identity_proofs: &[IdentityProof],
) -> Result<(), DatabaseTypeError> {
    let mut identities = HashSet::new();
    let is_unique = identity_proofs.iter()
        .map(|proof| proof.issuer.to_string())
        .all(|identity| identities.insert(identity));
    if !is_unique {
        // Identities must be unqiue
        return Err(DatabaseTypeError);
    };
    Ok(())
}
