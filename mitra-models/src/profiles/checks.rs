use std::collections::HashSet;

use crate::database::DatabaseTypeError;

use super::types::{
    DbActorKey,
    IdentityProof,
    PaymentOption,
    PaymentType,
};

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

pub fn check_payment_options(
    payment_options: &[PaymentOption],
    is_remote: bool,
) -> Result<(), DatabaseTypeError> {
    if !is_remote && payment_options.iter()
        .any(|option| matches!(
            option.payment_type(),
            PaymentType::Link | PaymentType::RemoteMoneroSubscription,
        ))
    {
        return Err(DatabaseTypeError);
    };
    if is_remote && payment_options.iter()
        .any(|option| matches!(
            option.payment_type(),
            PaymentType::EthereumSubscription | PaymentType::MoneroSubscription,
        ))
    {
        return Err(DatabaseTypeError);
    };
    // Uniqueness checks
    let mut types = HashSet::new();
    let is_unique = payment_options.iter()
        .filter_map(|option| match option {
            PaymentOption::Link(_) => None,
            _ => Some(i16::from(&option.payment_type())),
        })
        .all(|payment_type| types.insert(payment_type));
    if !is_unique {
        // Payment types must be unique
        return Err(DatabaseTypeError);
    };
    let mut chain_ids = HashSet::new();
    let is_unique = payment_options.iter()
        .filter_map(|option| match option {
            PaymentOption::Link(_) => None,
            PaymentOption::EthereumSubscription(info) =>
                Some(info.chain_id.to_string()),
            PaymentOption::MoneroSubscription(info) =>
                Some(info.chain_id.to_string()),
            PaymentOption::RemoteMoneroSubscription(info) =>
                Some(info.chain_id.to_string()),
        })
        .all(|chain_id| chain_ids.insert(chain_id));
    if !is_unique {
        // Chain IDs must be unique
        return Err(DatabaseTypeError);
    };
    Ok(())
}
