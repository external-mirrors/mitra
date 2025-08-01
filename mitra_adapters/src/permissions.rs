use chrono::{TimeDelta, Utc};
use uuid::Uuid;

use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    posts::queries::get_conversation_participants,
    profiles::{
        queries::get_profiles_by_ids,
        types::{MentionPolicy, DbActorProfile},
    },
    relationships::{
        queries::get_relationships,
        types::RelationshipType,
    },
};

const ACTOR_PROFILE_AGE_MIN: i64 = 60; // minutes

async fn is_connected(
    db_client: &impl DatabaseClient,
    source_id: Uuid,
    target_id: Uuid,
) -> Result<bool, DatabaseError> {
    let relationships =
        get_relationships(db_client, source_id, target_id).await?;
    for relationship in relationships {
        match relationship.relationship_type {
            RelationshipType::Follow => return Ok(true),
            RelationshipType::Subscription => return Ok(true),
            _ => (),
        };
    };
    Ok(false)
}

pub async fn filter_mentions(
    db_client: &impl DatabaseClient,
    mentions: Vec<Uuid>,
    author: &DbActorProfile,
    maybe_in_reply_to_id: Option<Uuid>,
) -> Result<Vec<DbActorProfile>, DatabaseError> {
    let profiles = get_profiles_by_ids(db_client, &mentions).await?;

    // Conversation participants should not be removed
    let participants = if let Some(in_reply_to_id) = maybe_in_reply_to_id {
        get_conversation_participants(db_client, in_reply_to_id).await?
    } else {
        vec![]
    };
    let is_participant = |profile_id: Uuid| {
        participants.iter().any(|participant| participant.id == profile_id)
    };

    let mut filtered = vec![];
    // TODO: optimize database queries
    for profile in profiles {
        if !profile.is_local() {
            // Don't filter remote mentions
            filtered.push(profile);
            continue;
        };
        let is_mention_allowed = match profile.mention_policy {
            MentionPolicy::None => true,
            MentionPolicy::OnlyKnown => {
                let age = Utc::now() - author.created_at;
                is_participant(profile.id) ||
                // Mentions from connections are always accepted
                is_connected(db_client, author.id, profile.id).await? ||
                    age >= TimeDelta::minutes(ACTOR_PROFILE_AGE_MIN)
            },
            MentionPolicy::OnlyContacts => {
                is_participant(profile.id) ||
                is_connected(db_client, author.id, profile.id).await?
            },
        };
        if !is_mention_allowed {
            log::warn!(
                "removing mention of {} made by {}",
                profile,
                author,
            );
            continue;
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
            queries::update_profile,
            types::{MentionPolicy, ProfileUpdateData},
            test_utils::{
                create_test_local_profile,
                create_test_remote_profile,
            },
        },
    };
    use super::*;

    #[tokio::test]
    #[serial]
    async fn test_filter_mentions_none() {
        let db_client = &mut create_test_database().await;
        let profile = create_test_local_profile(db_client, "test").await;
        let profile_data = ProfileUpdateData {
            mention_policy: MentionPolicy::None,
            ..ProfileUpdateData::from(&profile)
        };
        let (profile, _) = update_profile(
            db_client,
            profile.id,
            profile_data,
        ).await.unwrap();
        let author = create_test_remote_profile(
            db_client,
            "author",
            "social.example",
            "https://social.example/actor",
        ).await;

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
        let profile = create_test_local_profile(db_client, "test").await;
        let profile_data = ProfileUpdateData {
            mention_policy: MentionPolicy::OnlyKnown,
            ..ProfileUpdateData::from(&profile)
        };
        let (profile, _) = update_profile(
            db_client,
            profile.id,
            profile_data,
        ).await.unwrap();
        // New profile, no relationships
        let author = create_test_remote_profile(
            db_client,
            "author",
            "social.example",
            "https://social.example/actor",
        ).await;

        let filtered = filter_mentions(
            db_client,
            vec![profile.id],
            &author,
            None,
        ).await.unwrap();
        assert_eq!(filtered.len(), 0);
    }
}
