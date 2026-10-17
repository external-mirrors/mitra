use apx_sdk::{
    core::url::canonical::NonCanonicalUri,
    deserialization::deserialize_into_object_id,
};
use serde::Deserialize;
use serde_json::{Value as JsonValue};

use mitra_models::{
    database::{
        db_client_await,
        get_database_client,
        DatabaseConnectionPool,
        DatabaseError,
    },
    groups::helpers::{can_add_remove_admins, remove_group_admin},
    notifications::helpers::{
        create_subscription_expiration_notification,
    },
    posts::queries::{
        get_remote_post_by_object_id,
        set_pinned_flag,
    },
    profiles::queries::get_remote_profile_by_actor_id,
    relationships::queries::unsubscribe,
};
use mitra_validators::errors::ValidationError;

use crate::{
    adapters::users::get_actor_data,
    authority::Authority,
    identifiers::canonicalize_id,
    importers::{
        get_user_by_actor_id,
        ActorIdResolver,
        ApClient,
    },
};

use super::{Descriptor, HandlerResult};

#[derive(Deserialize)]
struct Remove {
    actor: String,

    #[serde(deserialize_with = "deserialize_into_object_id")]
    object: String,
    #[serde(deserialize_with = "deserialize_into_object_id")]
    target: String,

    audience: Option<NonCanonicalUri>,
}

pub async fn handle_remove(
    ap_client: &ApClient,
    db_pool: &DatabaseConnectionPool,
    activity: JsonValue,
) -> HandlerResult {
    let remove: Remove = serde_json::from_value(activity)?;
    let actor_profile = get_remote_profile_by_actor_id(
        db_client_await!(db_pool),
        &remove.actor,
    ).await?;
    let actor = actor_profile.actor_json
        .expect("actor data should be present");
    if Some(remove.target.clone()) == actor.subscribers {
        // Removing from subscribers
        let canonical_object_id = canonicalize_id(&remove.object)?;
        let authority = Authority::from(&ap_client.instance);
        let db_client = &mut **get_database_client(db_pool).await?;
        let user = get_user_by_actor_id(
            db_client,
            &authority,
            &canonical_object_id,
        ).await?;
        // actor is recipient, user is sender
        match unsubscribe(db_client, user.id, actor_profile.id).await {
            Ok(_) => {
                create_subscription_expiration_notification(
                    db_client,
                    actor_profile.id,
                    user.id,
                ).await?;
                return Ok(Some(Descriptor::target("subscribers")));
            },
            // Ignore removal if relationship does not exist
            Err(DatabaseError::NotFound(_)) => return Ok(None),
            Err(other_error) => return Err(other_error.into()),
        };
    };
    if Some(remove.target.clone()) == actor.featured {
        // Remove from featured
        let db_client = &**get_database_client(db_pool).await?;
        let post = match get_remote_post_by_object_id(
            db_client,
            &remove.object,
        ).await {
            Ok(post) => post,
            Err(DatabaseError::NotFound(_)) => return Ok(None),
            Err(other_error) => return Err(other_error.into()),
        };
        set_pinned_flag(db_client, post.id, false).await?;
        return Ok(Some(Descriptor::target("featured")));
    };
    if let Some(audience) = remove.audience {
        // FEP-1b12 `Remove`
        let group = ActorIdResolver::default().resolve(
            ap_client,
            db_pool,
            &audience.to_string(),
        ).await?;
        let authority = Authority::from(&ap_client.instance);
        let group_data = get_actor_data(authority.root(), &group);
        if group_data.fep_1b12_administrators.as_ref()
            .is_some_and(|id| id.to_string() == remove.target)
        {
            let member = ActorIdResolver::default().resolve(
                ap_client,
                db_pool,
                &remove.object,
            ).await?;
            let db_client = &**get_database_client(db_pool).await?;
            if !can_add_remove_admins(db_client, actor_profile.id, group.id).await? {
                return Err(ValidationError("actor doesn't have a permission to remove admins").into());
            };
            match remove_group_admin(db_client, member.id, group.id).await {
                Ok(_) => return Ok(Some(Descriptor::target("moderators"))),
                Err(DatabaseError::NotFound(_)) => return Ok(None),
                Err(other_error) => return Err(other_error.into()),
            };
        };
    };
    log::warn!("unknown target: {}", remove.target);
    Ok(None)
}
