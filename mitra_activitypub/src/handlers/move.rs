/// https://codeberg.org/fediverse/fep/src/branch/main/fep/7628/fep-7628.md
use serde::Deserialize;
use serde_json::Value;

use mitra_config::Config;
use mitra_models::{
    database::DatabaseClient,
    notifications::helpers::create_move_notification,
    profiles::helpers::find_verified_aliases,
    relationships::queries::{
        get_followers,
        unfollow,
    },
    users::queries::get_user_by_id,
};
use mitra_services::media::MediaStorage;
use mitra_validators::errors::ValidationError;

use crate::{
    builders::{
        follow::follow_or_create_request,
        undo_follow::prepare_undo_follow,
    },
    identifiers::profile_actor_id,
    importers::ActorIdResolver,
    vocabulary::PERSON,
};

use super::HandlerResult;

#[derive(Deserialize)]
struct Move {
    actor: String,
    object: String,
    target: String,
}

pub async fn handle_move(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    activity: Value,
) -> HandlerResult {
    // Move(Person)
    let activity: Move = serde_json::from_value(activity)
        .map_err(|_| ValidationError("unexpected activity structure"))?;
    // Mastodon (push mode): actor is old profile (object)
    // Mitra (pull mode): actor is new profile (target)
    if activity.object != activity.actor && activity.target != activity.actor {
        return Err(ValidationError("actor ID mismatch").into());
    };

    let instance = config.instance();
    let storage = MediaStorage::from(config);

    let old_profile = ActorIdResolver::default().resolve(
        db_client,
        &instance,
        &storage,
        &activity.object,
    ).await?;
    let old_actor_id = profile_actor_id(&instance.url(), &old_profile);

    let new_profile = ActorIdResolver::default().force_refetch().resolve(
        db_client,
        &instance,
        &storage,
        &activity.target,
    ).await?;

    // Find aliases by DIDs (verified)
    let mut aliases = find_verified_aliases(db_client, &new_profile).await?
        .into_iter()
        .map(|profile| profile_actor_id(&instance.url(), &profile))
        .collect::<Vec<_>>();
    // Add aliases reported by server (actor's alsoKnownAs property)
    aliases.extend(new_profile.aliases.clone().into_actor_ids());
    if !aliases.contains(&old_actor_id) {
        return Err(ValidationError("target ID is not an alias").into());
    };

    let followers = get_followers(db_client, &old_profile.id).await?;
    for follower in followers {
        if !follower.is_local() {
            // Push mode: old actor is remote, so all followers are local
            // Pull mode: ignore remote followers if old actor is local
            continue;
        };
        let follower = get_user_by_id(db_client, &follower.id).await?;
        // Unfollow old profile
        let maybe_follow_request_deleted = unfollow(
            db_client,
            &follower.id,
            &old_profile.id,
        ).await?;
        // Send Undo(Follow) if old actor is not local
        if let Some(ref old_actor) = old_profile.actor_json {
            let (follow_request_id, follow_request_has_deprecated_ap_id) =
                maybe_follow_request_deleted
                    .expect("follow request must exist");
            prepare_undo_follow(
                &instance,
                &follower,
                old_actor,
                follow_request_id,
                follow_request_has_deprecated_ap_id,
            ).enqueue(db_client).await?;
        };
        if follower.id == new_profile.id {
            // Don't self-follow
            continue;
        };
        // Follow new profile
        follow_or_create_request(
            db_client,
            &instance,
            &follower,
            &new_profile,
        ).await?;
        create_move_notification(
            db_client,
            new_profile.id,
            follower.id,
        ).await?;
    };

    Ok(Some(PERSON))
}
