use apx_sdk::core::url::canonical::{
    CanonicalUri,
    NonCanonicalUri,
};

use mitra_models::{
    activitypub::queries::expand_collections,
    database::{DatabaseClient, DatabaseError},
    profiles::{
        queries::get_remote_profiles_by_actor_ids,
        types::DbActorProfile,
    },
};

use crate::deliverer::Recipient;

/// Returns remote recipients of the activity
pub async fn get_activity_audience_actors(
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

pub async fn get_activity_recipients(
    db_client: &impl DatabaseClient,
    audience: &[CanonicalUri],
) -> Result<Vec<Recipient>, DatabaseError> {
    let profiles = get_activity_audience_actors(
        db_client,
        audience,
    ).await?;
    let recipients = profiles
        .into_iter()
        .filter_map(|profile| profile.actor_json)
        .flat_map(|actor_data| Recipient::for_inbox(&actor_data))
        .collect();
    Ok(recipients)
}

pub enum EndpointType {
    Inbox,
    Outbox,
}

#[expect(async_fn_in_trait)]
pub trait Deliverable {
    fn to(&self) -> &[NonCanonicalUri];
    fn cc(&self) -> &[NonCanonicalUri];

    async fn get_recipients(
        &self,
        db_client: &impl DatabaseClient,
    ) -> Result<Vec<Recipient>, DatabaseError> {
        let audience: Vec<_> = self
            .to()
            .iter()
            .chain(self.cc())
            .map(|id| id.clone().into_canonical())
            .collect();
        get_activity_recipients(db_client, &audience).await
    }
}

#[cfg(test)]
mod tests {
    use serial_test::serial;
    use mitra_models::{
        database::test_utils::create_test_database,
        profiles::test_utils::create_test_remote_profile,
    };
    use super::*;

    #[tokio::test]
    #[serial]
    async fn test_get_activity_audience_actors() {
        let db_client = &mut create_test_database().await;
        let actor_1 = "https://social.example/actors/1";
        let actor_2 = "https://social.example/actors/2";
        let actor_3 = "https://social.example/actors/3";
        let profile_1 = create_test_remote_profile(
            db_client,
            "test_1",
            "social.example",
            actor_1,
        ).await;
        let profile_2 = create_test_remote_profile(
            db_client,
            "test_2",
            "social.example",
            actor_2,
        ).await;
        let profile_3 = create_test_remote_profile(
            db_client,
            "test_3",
            "social.example",
            actor_3,
        ).await;
        let audience: Vec<_> = [actor_2, actor_3, actor_1]
            .into_iter()
            .map(|id| CanonicalUri::parse_canonical(id).unwrap())
            .collect();
        let recipients = get_activity_audience_actors(
            db_client,
            &audience,
        ).await.unwrap();
        assert_eq!(recipients.len(), 3);
        assert_eq!(recipients[0].id, profile_2.id);
        assert_eq!(recipients[1].id, profile_3.id);
        assert_eq!(recipients[2].id, profile_1.id);
    }
}
