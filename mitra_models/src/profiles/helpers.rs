use std::str::FromStr;

use uuid::Uuid;

use crate::database::{DatabaseClient, DatabaseError};

use super::queries::{
    get_profile_by_acct,
    get_profile_by_id,
    get_profile_by_remote_actor_id,
    search_profiles_by_did_only,
};
use super::types::DbActorProfile;

pub async fn get_profile_by_id_or_acct(
    db_client: &impl DatabaseClient,
    profile_id_or_acct: &str,
) -> Result<DbActorProfile, DatabaseError> {
    // Only remote profiles could have usernames that are valid UUIDs
    if let Ok(profile_id) = Uuid::from_str(profile_id_or_acct) {
        let profile = get_profile_by_id(db_client, &profile_id).await?;
        Ok(profile)
    } else {
        let profile = get_profile_by_acct(db_client, profile_id_or_acct).await?;
        Ok(profile)
    }
}

pub async fn find_declared_aliases(
    db_client: &impl DatabaseClient,
    profile: &DbActorProfile,
) -> Result<Vec<(String, Option<DbActorProfile>)>, DatabaseError> {
    let mut results = vec![];
    for actor_id in profile.aliases.clone().into_actor_ids() {
        let maybe_profile = match get_profile_by_remote_actor_id(
            db_client,
            &actor_id,
        ).await {
            Ok(profile) => Some(profile),
            // Unknown or local actor
            Err(DatabaseError::NotFound(_)) => None,
            Err(other_error) => return Err(other_error),
        };
        results.push((actor_id, maybe_profile));
    };
    Ok(results)
}

pub async fn find_verified_aliases(
    db_client: &impl DatabaseClient,
    profile: &DbActorProfile,
) -> Result<Vec<DbActorProfile>, DatabaseError> {
    let mut results = vec![];
    for identity_proof in profile.identity_proofs.inner() {
        let aliases = search_profiles_by_did_only(
            db_client,
            &identity_proof.issuer,
        ).await?;
        for alias in aliases {
            if alias.id == profile.id {
                continue;
            };
            results.push(alias);
        };
    };
    Ok(results)
}
