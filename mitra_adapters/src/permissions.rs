use chrono::{Duration, Utc};
use uuid::Uuid;

use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    profiles::{
        queries::get_profiles_by_ids,
        types::{MentionPolicy, DbActorProfile},
    },
    relationships::{
        queries::has_relationship,
        types::RelationshipType,
    },
};

const ACTOR_PROFILE_AGE_MIN: i64 = 60; // minutes

async fn is_connected(
    db_client: &impl DatabaseClient,
    source_id: &Uuid,
    target_id: &Uuid,
) -> Result<bool, DatabaseError> {
    let is_follower = has_relationship(
        db_client,
        source_id,
        target_id,
        RelationshipType::Follow,
    ).await?;
    let is_followee = has_relationship(
        db_client,
        target_id,
        source_id,
        RelationshipType::Follow,
    ).await?;
    let is_subscriber = has_relationship(
        db_client,
        source_id,
        target_id,
        RelationshipType::Subscription,
    ).await?;
    let is_subscribee = has_relationship(
        db_client,
        target_id,
        source_id,
        RelationshipType::Subscription,
    ).await?;
    Ok(is_follower || is_followee || is_subscriber || is_subscribee)
}

pub async fn filter_mentions(
    db_client: &impl DatabaseClient,
    mentions: Vec<Uuid>,
    author: &DbActorProfile,
    maybe_in_reply_to_author: Option<&DbActorProfile>,
) -> Result<Vec<DbActorProfile>, DatabaseError> {
    let profiles = get_profiles_by_ids(db_client, mentions.clone()).await?;
    if profiles.len() != mentions.len() {
        return Err(DatabaseError::NotFound("profile"));
    };

    if let Some(in_reply_to_author) = maybe_in_reply_to_author {
        if in_reply_to_author.id != author.id {
            // Don't filter mentions in reply, unless it's a self-reply
            return Ok(profiles);
        };
    };
    let mut filtered = vec![];
    // TODO: optimize database queries
    for profile in profiles {
        if !profile.is_local() {
            // Don't filter remote mentions
            filtered.push(profile);
            continue;
        };
        match profile.mention_policy {
            MentionPolicy::None => (),
            MentionPolicy::OnlyKnown => {
                let age = Utc::now() - author.created_at;
                // Mentions from connections are always accepted
                if !is_connected(db_client, &author.id, &profile.id).await? &&
                    age < Duration::minutes(ACTOR_PROFILE_AGE_MIN)
                {
                    log::warn!("mention removed from post");
                    continue;
                };
            },
        };
        filtered.push(profile);
    };
    Ok(filtered)
}

#[cfg(test)]
mod tests {
    use serial_test::serial;
    use mitra_models::{
        database::test_utils::create_test_database,
        profiles::{
            queries::create_profile,
            types::{MentionPolicy, ProfileCreateData},
        },
    };
    use super::*;

    #[tokio::test]
    #[serial]
    async fn test_filter_mentions_none() {
        let db_client = &mut create_test_database().await;
        let profile_data = ProfileCreateData {
            username: "test".to_string(),
            mention_policy: MentionPolicy::None,
            ..Default::default()
        };
        let profile = create_profile(
            db_client,
            profile_data,
        ).await.unwrap();
        let author_data = ProfileCreateData {
            username: "author".to_string(),
            ..Default::default()
        };
        let author = create_profile(
            db_client,
            author_data,
        ).await.unwrap();

        let filtered = filter_mentions(
            db_client,
            vec![profile.id],
            &author,
            None,
        ).await.unwrap();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id, profile.id);
    }

    #[tokio::test]
    #[serial]
    async fn test_filter_mentions_only_known() {
        let db_client = &mut create_test_database().await;
        let profile_data = ProfileCreateData {
            username: "test".to_string(),
            mention_policy: MentionPolicy::OnlyKnown,
            ..Default::default()
        };
        let profile = create_profile(
            db_client,
            profile_data,
        ).await.unwrap();
        // New profile, no relationships
        let author_data = ProfileCreateData {
            username: "author".to_string(),
            ..Default::default()
        };
        let author = create_profile(
            db_client,
            author_data,
        ).await.unwrap();

        let filtered = filter_mentions(
            db_client,
            vec![profile.id],
            &author,
            None,
        ).await.unwrap();
        assert_eq!(filtered.len(), 0);
    }
}
