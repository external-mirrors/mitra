use mitra_config::Instance;
use mitra_models::{
    accounts::types::User,
    database::{DatabaseClient, DatabaseError},
    notifications::helpers::create_follow_request_notification,
    profiles::types::DbActorProfile,
    relationships::{
        helpers::create_follow_request,
        queries::follow,
    },
};

use crate::builders::follow::prepare_follow;

pub async fn follow_or_create_request(
    db_client: &mut impl DatabaseClient,
    instance: &Instance,
    current_user: &User,
    target_profile: &DbActorProfile,
) -> Result<(), DatabaseError> {
    if target_profile.manually_approves_followers || !target_profile.is_local() {
        // Create follow request if target requires approval or it is remote
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
                } else {
                    create_follow_request_notification(
                        db_client,
                        current_user.id,
                        target_profile.id,
                    ).await?;
                };
            },
            // Do nothing if request has already been sent,
            // or if already following
            Err(DatabaseError::AlreadyExists(_)) => (),
            Err(other_error) => return Err(other_error),
        };
    } else {
        match follow(db_client, current_user.id, target_profile.id).await {
            Ok(_) => (),
            Err(DatabaseError::AlreadyExists(_)) => (), // already following
            Err(other_error) => return Err(other_error),
        };
    };
    Ok(())
}
