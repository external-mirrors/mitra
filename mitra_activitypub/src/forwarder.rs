use apx_sdk::core::url::canonical::CanonicalUri;

use mitra_models::{
    activitypub::queries::expand_collections,
    database::{DatabaseClient, DatabaseError},
    profiles::{
        queries::get_remote_profiles_by_actor_ids,
        types::DbActorProfile,
    },
};

/// Returns remote recipients of the activity
pub async fn get_activity_recipients(
    db_client: &impl DatabaseClient,
    audience: &[CanonicalUri],
) -> Result<Vec<DbActorProfile>, DatabaseError> {
    let expanded_audience = expand_collections(
        db_client,
        audience,
    ).await?;
    let recipients = get_remote_profiles_by_actor_ids(
        db_client,
        &expanded_audience,
    ).await?;
    Ok(recipients)
}

pub enum EndpointType {
    Inbox,
    Outbox,
}
