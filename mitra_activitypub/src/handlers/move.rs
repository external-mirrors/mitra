/// https://codeberg.org/fediverse/fep/src/branch/main/fep/7628/fep-7628.md
use serde::Deserialize;
use serde_json::Value;

use mitra_models::{
    accounts::queries::get_user_by_id,
    database::{
        get_database_client,
        DatabaseConnectionPool,
    },
    notifications::helpers::create_move_notification,
    profiles::helpers::find_verified_aliases,
    relationships::queries::{
        get_followers,
        unfollow,
    },
};
use mitra_validators::errors::ValidationError;

use crate::{
    adapters::follow_requests::follow_or_create_request,
    authority::Authority,
    builders::undo_follow::prepare_undo_follow,
    identifiers::profile_actor_id,
    importers::{ActorIdResolver, ApClient},
};

use super::{Descriptor, HandlerResult};

#[derive(Deserialize)]
struct Move {
    actor: String,
    object: String,
    target: String,
}

enum MoveSubtype {
    Push,
    Pull,
}

pub async fn handle_move(
    ap_client: &ApClient,
    db_pool: &DatabaseConnectionPool,
    activity: Value,
) -> HandlerResult {
    // Move(Person)
    let activity: Move = serde_json::from_value(activity)?;
    // Mastodon (push mode): actor is old profile (object)
    // Mitra (pull mode): actor is new profile (target)
    let subtype = if activity.actor == activity.object {
        MoveSubtype::Push
    } else if activity.actor == activity.target {
        MoveSubtype::Pull
    } else {
        return Err(ValidationError("actor ID mismatch").into());
    };

    let instance = &ap_client.instance;
    let authority = Authority::from(instance);

    let old_profile = ActorIdResolver::default().resolve(
        ap_client,
        db_pool,
        &activity.object,
    ).await?;
    let old_actor_id = profile_actor_id(&authority, &old_profile);

    let new_profile = ActorIdResolver::default().force_refetch().resolve(
        ap_client,
        db_pool,
        &activity.target,
    ).await?;

    let db_client = &mut **get_database_client(db_pool).await?;
    match subtype {
        MoveSubtype::Push => {
            // Find aliases reported by server (actor's alsoKnownAs property)
            if !new_profile.aliases.contains(&old_actor_id) {
                return Err(ValidationError("target ID is not an alias").into());
            };
        },
        MoveSubtype::Pull => {
            // Find aliases by DIDs (verified)
            let aliases = find_verified_aliases(db_client, &new_profile).await?
                .into_iter()
                .map(|profile| profile_actor_id(&authority, &profile))
                .collect::<Vec<_>>();
            if !aliases.contains(&old_actor_id) {
                return Err(ValidationError("object ID is not an alias").into());
            };
        },
    };

    let followers = get_followers(db_client, old_profile.id).await?;
    for follower in followers {
        if !follower.is_local() {
            // Push mode: old actor is remote, so all followers are local
            // Pull mode: ignore remote followers if old actor is local
            continue;
        };
        let follower = get_user_by_id(db_client, follower.id).await?;
        // Unfollow old profile
        let maybe_follow_request_deleted = unfollow(
            db_client,
            follower.id,
            old_profile.id,
        ).await?;
        // Send Undo(Follow) if old actor is not local
        if let Some(ref old_actor) = old_profile.actor_json {
            let (follow_request_id, follow_request_has_deprecated_ap_id) =
                maybe_follow_request_deleted
                    .expect("follow request must exist");
            prepare_undo_follow(
                db_client,
                instance,
                &follower,
                old_actor,
                follow_request_id,
                follow_request_has_deprecated_ap_id,
            ).await?.enqueue(db_client).await?;
        };
        if follower.id == new_profile.id {
            // Don't self-follow
            continue;
        };
        // Follow new profile
        follow_or_create_request(
            db_client,
            instance,
            &follower,
            &new_profile,
        ).await?;
        create_move_notification(
            db_client,
            new_profile.id,
            follower.id,
        ).await?;
    };

    Ok(Some(Descriptor::object("Actor")))
}
