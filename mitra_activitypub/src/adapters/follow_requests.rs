use uuid::Uuid;

use mitra_config::Instance;
use mitra_models::{
    accounts::types::User,
    activitypub::queries::add_relationship,
    database::{DatabaseClient, DatabaseError},
    notifications::helpers::create_follow_request_notification,
    profiles::types::DbActorProfile,
    relationships::{
        helpers::create_follow_request,
        queries::{
            follow_request_accepted,
            get_relationship_by_id,
        },
        types::RelationshipDetailed,
    },
};

use crate::{
    adapters::users::get_actor_data,
    authority::{Authority, AuthorityRoot},
    builders::follow::prepare_follow,
    utils::parse_id_from_db_lenient,
};

pub async fn accept_and_add_follower(
    authority_root: &AuthorityRoot,
    db_client: &mut impl DatabaseClient,
    follow_request_id: Uuid,
) -> Result<(), DatabaseError> {
    let mut transaction = db_client.transaction().await?;
    let relationship_id =
        follow_request_accepted(&mut transaction, follow_request_id).await?;
    let RelationshipDetailed { source, target, .. } =
        get_relationship_by_id(&transaction, relationship_id).await?;
    let source_actor_data = get_actor_data(authority_root, &source);
    let target_actor_data = get_actor_data(authority_root, &target);
    if let Some(followers_id) = target_actor_data.followers {
        let canonical_followers_id =
            parse_id_from_db_lenient(&followers_id)?;
        let canonical_follower_id =
            parse_id_from_db_lenient(&source_actor_data.id)?;
        match add_relationship(
            &mut transaction,
            target.id, // collection owner
            relationship_id,
            &canonical_followers_id,
            &canonical_follower_id,
        ).await {
            Ok(_) => (),
            Err(DatabaseError::NotFound(_)) =>
                log::warn!(
                    "can't update followers collection: actor not found ({})",
                    canonical_follower_id,
                ),
            Err(other_error) => return Err(other_error),
        };
    } else {
        // No `followers` collection, no delivery.
        // Local actors always have a `followers` collection.
        log::warn!("actor doesn't have followers collection");
    };
    transaction.commit().await?;
    Ok(())
}

pub async fn follow_or_create_request(
    db_client: &mut impl DatabaseClient,
    instance: &Instance,
    current_user: &User,
    target_profile: &DbActorProfile,
) -> Result<(), DatabaseError> {
    match create_follow_request(
        db_client,
        current_user.id,
        target_profile.id,
    ).await {
        Ok(follow_request) => {
            if let Some(ref remote_actor) = target_profile.actor_json {
                prepare_follow(
                    instance,
                    current_user,
                    remote_actor,
                    follow_request.id,
                )?.save_and_enqueue(db_client).await?;
            } else if target_profile.manually_approves_followers {
                create_follow_request_notification(
                    db_client,
                    current_user.id,
                    target_profile.id,
                ).await?;
            } else {
                // Auto-accept if local profile is not locked
                let authority = Authority::from(instance);
                accept_and_add_follower(
                    authority.root(),
                    db_client,
                    follow_request.id,
                ).await?;
            };
        },
        // Do nothing if request has already been sent,
        // or if already following
        Err(DatabaseError::AlreadyExists(_)) => (),
        Err(other_error) => return Err(other_error),
    };
    Ok(())
}

#[cfg(test)]
mod tests {
    use serial_test::serial;
    use mitra_models::{
        accounts::test_utils::create_test_user,
        database::test_utils::create_test_database,
        relationships::{
            helpers::create_follow_request,
            queries::has_relationship,
            types::RelationshipType,
        },
    };
    use super::*;

    #[tokio::test]
    #[serial]
    async fn test_accept_and_add_follower() {
        let db_client = &mut create_test_database().await;
        let source = create_test_user(db_client, "source").await;
        let target = create_test_user(db_client, "target").await;
        let authority = Authority::server_unchecked("https://social.example");
        let follow_request = create_follow_request(
            db_client,
            source.id,
            target.id,
        ).await.unwrap();
        // Should succeed even if actor JSON is not stored
        accept_and_add_follower(
            authority.root(),
            db_client,
            follow_request.id,
        ).await.unwrap();
        let is_follower = has_relationship(
            db_client,
            source.id,
            target.id,
            RelationshipType::Follow,
        ).await.unwrap();
        assert!(is_follower);
    }
}
