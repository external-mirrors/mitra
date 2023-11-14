use std::collections::HashMap;

use chrono::{Duration, Utc};
use serde_json::{Value as JsonValue};

use mitra_config::{Config, Instance};
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    posts::helpers::get_local_post_by_id,
    posts::queries::get_post_by_remote_object_id,
    posts::types::Post,
    profiles::queries::{
        get_profile_by_acct,
        get_profile_by_remote_actor_id,
    },
    profiles::types::DbActorProfile,
};
use mitra_utils::urls::guess_protocol;
use mitra_validators::errors::ValidationError;

use crate::activitypub::{
    actors::helpers::{create_remote_profile, update_remote_profile},
    agent::FederationAgent,
    constants::AP_CONTEXT,
    deserialization::{find_object_id, parse_into_id_array},
    handlers::create::{get_object_links, handle_note},
    identifiers::parse_local_object_id,
    receiver::{handle_activity, HandlerError},
    types::Object,
    vocabulary::GROUP,
};
use crate::media::MediaStorage;
use crate::webfinger::types::{ActorAddress, JsonResourceDescriptor};

use super::fetchers::{
    fetch_actor,
    fetch_collection,
    fetch_json,
    fetch_object,
    FetchError,
};

async fn import_profile(
    db_client: &mut impl DatabaseClient,
    instance: &Instance,
    storage: &MediaStorage,
    actor_id: &str,
) -> Result<DbActorProfile, HandlerError> {
    let agent = FederationAgent::new(instance);
    let actor = fetch_actor(&agent, actor_id).await?;
    let actor_address = actor.address()?;
    let acct = actor_address.acct(&instance.hostname());
    // 'acct' is the primary identifier
    let profile = match get_profile_by_acct(db_client, &acct).await {
        Ok(profile) => {
            // WARNING: Possible actor ID change
            log::info!("re-fetched profile {}", profile.acct);
            let profile_updated = update_remote_profile(
                db_client,
                instance,
                storage,
                profile,
                actor,
            ).await?;
            profile_updated
        },
        Err(DatabaseError::NotFound(_)) => {
            log::info!("fetched profile {}", acct);
            let profile = create_remote_profile(
                db_client,
                instance,
                storage,
                actor,
            ).await?;
            profile
        },
        Err(other_error) => return Err(other_error.into()),
    };
    Ok(profile)
}

async fn refresh_remote_profile(
    db_client: &mut impl DatabaseClient,
    instance: &Instance,
    storage: &MediaStorage,
    profile: DbActorProfile,
) -> Result<DbActorProfile, HandlerError> {
    let agent = FederationAgent::new(instance);
    let actor_id = &profile.actor_json.as_ref()
        .expect("actor data should be present")
        .id;
    let profile = if profile.updated_at < Utc::now() - Duration::days(1) {
        // Try to re-fetch actor profile
        match fetch_actor(&agent, actor_id).await {
            Ok(actor) => {
                log::info!("re-fetched profile {}", profile.acct);
                let profile_updated = update_remote_profile(
                    db_client,
                    instance,
                    storage,
                    profile,
                    actor,
                ).await?;
                profile_updated
            },
            Err(err) => {
                // Ignore error and return stored profile
                log::warn!(
                    "failed to re-fetch {} ({})", profile.acct, err,
                );
                profile
            },
        }
    } else {
        // Refresh is not needed
        profile
    };
    Ok(profile)
}

pub async fn get_or_import_profile_by_actor_id(
    db_client: &mut impl DatabaseClient,
    instance: &Instance,
    storage: &MediaStorage,
    actor_id: &str,
) -> Result<DbActorProfile, HandlerError> {
    if actor_id.starts_with(&instance.url()) {
        return Err(HandlerError::LocalObject);
    };
    let profile = match get_profile_by_remote_actor_id(
        db_client,
        actor_id,
    ).await {
        Ok(profile) => {
            refresh_remote_profile(
                db_client,
                instance,
                storage,
                profile,
            ).await?
        },
        Err(DatabaseError::NotFound(_)) => {
            import_profile(db_client, instance, storage, actor_id).await?
        },
        Err(other_error) => return Err(other_error.into()),
    };
    Ok(profile)
}

impl JsonResourceDescriptor {
    fn find_actor_id(&self) -> Option<String> {
        // Lemmy servers can have Group and Person actors with the same name
        // https://github.com/LemmyNet/lemmy/issues/2037
        let ap_type_property = format!("{}#type", AP_CONTEXT);
        let group_link = self.links.iter()
            .find(|link| {
                link.rel == "self" &&
                link.properties
                    .get(&ap_type_property)
                    .map(|val| val.as_str()) == Some(GROUP)
            });
        let link = if let Some(link) = group_link {
            // Prefer Group if the actor type is provided
            link
        } else {
            // Otherwise take first "self" link
            self.links.iter().find(|link| link.rel == "self")?
        };
        let actor_id = link.href.as_ref()?.to_string();
        Some(actor_id)
    }
}

async fn perform_webfinger_query(
    agent: &FederationAgent,
    actor_address: &ActorAddress,
) -> Result<String, FetchError> {
    let webfinger_resource = actor_address.to_acct_uri();
    let webfinger_url = format!(
        "{}://{}/.well-known/webfinger",
        guess_protocol(&actor_address.hostname),
        actor_address.hostname,
    );
    let jrd: JsonResourceDescriptor = fetch_json(
        agent,
        &webfinger_url,
        &[("resource", &webfinger_resource)],
    ).await?;
    let actor_id = jrd.find_actor_id()
        .ok_or(FetchError::OtherError("actor ID not found"))?;
    Ok(actor_id)
}

pub async fn import_profile_by_actor_address(
    db_client: &mut impl DatabaseClient,
    instance: &Instance,
    storage: &MediaStorage,
    actor_address: &ActorAddress,
) -> Result<DbActorProfile, HandlerError> {
    if actor_address.hostname == instance.hostname() {
        return Err(HandlerError::LocalObject);
    };
    let agent = FederationAgent::new(instance);
    let actor_id = perform_webfinger_query(&agent, actor_address).await?;
    import_profile(db_client, instance, storage, &actor_id).await
}

// Works with local profiles
pub async fn get_or_import_profile_by_actor_address(
    db_client: &mut impl DatabaseClient,
    instance: &Instance,
    storage: &MediaStorage,
    actor_address: &ActorAddress,
) -> Result<DbActorProfile, HandlerError> {
    let acct = actor_address.acct(&instance.hostname());
    let profile = match get_profile_by_acct(
        db_client,
        &acct,
    ).await {
        Ok(profile) => {
            if actor_address.hostname == instance.hostname() {
                profile
            } else {
                refresh_remote_profile(
                    db_client,
                    instance,
                    storage,
                    profile,
                ).await?
            }
        },
        Err(db_error @ DatabaseError::NotFound(_)) => {
            if actor_address.hostname == instance.hostname() {
                return Err(db_error.into());
            };
            import_profile_by_actor_address(
                db_client,
                instance,
                storage,
                actor_address,
            ).await?
        },
        Err(other_error) => return Err(other_error.into()),
    };
    Ok(profile)
}

pub async fn get_post_by_object_id(
    db_client: &impl DatabaseClient,
    instance_url: &str,
    object_id: &str,
) -> Result<Post, DatabaseError> {
    match parse_local_object_id(instance_url, object_id) {
        Ok(post_id) => {
            // Local post
            let post = get_local_post_by_id(db_client, &post_id).await?;
            Ok(post)
        },
        Err(_) => {
            // Remote post
            let post = get_post_by_remote_object_id(db_client, object_id).await?;
            Ok(post)
        },
    }
}

const RECURSION_DEPTH_MAX: usize = 50;

pub async fn import_post(
    db_client: &mut impl DatabaseClient,
    instance: &Instance,
    storage: &MediaStorage,
    object_id: String,
    object_received: Option<Object>,
) -> Result<Post, HandlerError> {
    let agent = FederationAgent::new(instance);

    let mut queue = vec![object_id]; // LIFO queue
    let mut fetch_count = 0;
    let mut maybe_object = object_received;
    let mut objects: Vec<Object> = vec![];
    let mut redirects: HashMap<String, String> = HashMap::new();
    let mut posts = vec![];

    // Fetch ancestors by going through inReplyTo references
    // TODO: fetch replies too
    #[allow(clippy::while_let_loop)]
    loop {
        let object_id = match queue.pop() {
            Some(object_id) => {
                if objects.iter().any(|object| object.id == object_id) {
                    // Can happen due to redirections
                    log::warn!("loop detected");
                    continue;
                };
                if let Ok(post_id) = parse_local_object_id(&instance.url(), &object_id) {
                    if objects.is_empty() {
                        // Initial object must not be local
                        return Err(HandlerError::LocalObject);
                    };
                    // Object is a local post
                    // Verify post exists, return error if it doesn't
                    get_local_post_by_id(db_client, &post_id).await?;
                    continue;
                };
                match get_post_by_remote_object_id(
                    db_client,
                    &object_id,
                ).await {
                    Ok(post) => {
                        // Object already fetched
                        if objects.len() == 0 {
                            // Return post corresponding to initial object ID
                            return Ok(post);
                        };
                        continue;
                    },
                    Err(DatabaseError::NotFound(_)) => (),
                    Err(other_error) => return Err(other_error.into()),
                };
                object_id
            },
            None => {
                // No object to fetch
                break;
            },
        };
        let object = match maybe_object {
            Some(object) => object,
            None => {
                if fetch_count >= RECURSION_DEPTH_MAX {
                    // TODO: create tombstone
                    return Err(FetchError::RecursionError.into());
                };
                let object: Object = fetch_object(&agent, &object_id).await?;
                log::info!("fetched object {}", object.id);
                fetch_count +=  1;
                object
            },
        };
        if object.id != object_id {
            // ID of fetched object doesn't match requested ID
            // Add IDs to the map of redirects
            redirects.insert(object_id, object.id.clone());
            queue.push(object.id.clone());
            // Don't re-fetch object on the next iteration
            maybe_object = Some(object);
            continue;
        };
        if let Some(ref object_id) = object.in_reply_to {
            // Fetch parent object on next iteration
            queue.push(object_id.to_owned());
        };
        for object_id in get_object_links(&object) {
            // Fetch linked objects after fetching current thread
            queue.insert(0, object_id);
        };
        maybe_object = None;
        objects.push(object);
    };
    let initial_object_id = objects[0].id.clone();

    // Objects are ordered according to their place in reply tree,
    // starting with the root
    objects.reverse();
    for object in objects {
        let post = handle_note(
            db_client,
            instance,
            storage,
            object,
            &redirects,
        ).await?;
        posts.push(post);
    };

    let initial_post = posts.into_iter()
        .find(|post| post.object_id.as_ref() == Some(&initial_object_id))
        .unwrap();
    Ok(initial_post)
}

pub async fn import_from_outbox(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    actor_id: &str,
    limit: usize,
) -> Result<(), HandlerError> {
    let instance = config.instance();
    let agent = FederationAgent::new(&instance);
    let actor = fetch_actor(&agent, actor_id).await?;
    let activities =
        fetch_collection(&agent, &actor.outbox, limit).await?;
    log::info!("fetched {} activities", activities.len());
    // Outbox has reverse chronological order
    let activities = activities.into_iter().rev();
    for activity in activities {
        let activity_actor = find_object_id(&activity["actor"])
            .map_err(|_| ValidationError("invalid actor property"))?;
        if activity_actor != actor.id {
            log::warn!("activity doesn't belong to outbox owner");
            continue;
        };
        handle_activity(
            config,
            db_client,
            &activity,
            true, // is authenticated
        ).await.unwrap_or_else(|error| {
            log::warn!(
                "failed to process activity ({}): {}",
                error,
                activity,
            );
        });
    };
    Ok(())
}

pub async fn import_replies(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    object_id: &str,
    limit: usize,
) -> Result<(), HandlerError> {
    let instance = config.instance();
    let agent = FederationAgent::new(&instance);
    let storage = MediaStorage::from(config);
    let object: JsonValue = fetch_object(&agent, object_id).await?;
    let mut replies = vec![];
    match &object["replies"] {
        JsonValue::Null => (), // no replies
        JsonValue::String(collection_id) => {
            let items =
                fetch_collection(&agent, collection_id, limit).await?;
            for item in items {
                let object_id = find_object_id(&item)?;
                replies.push(object_id);
            };
        },
        value => {
            // Embedded collection
            let items = parse_into_id_array(&value["items"])?;
            replies.extend(items);
            // Embedded first page contains self-replies (Mastodon only)
            let items = parse_into_id_array(&value["first"]["items"])?;
            replies.extend(items);
            if let Some(next_page_url) = value["first"]["next"].as_str() {
                let next_page: JsonValue = fetch_object(&agent, next_page_url).await?;
                let items = parse_into_id_array(&next_page["items"])?;
                replies.extend(items);
            };
        },
    };
    let replies: Vec<_> = replies.into_iter()
        .take(limit)
        .collect();
    log::info!("found {} replies", replies.len());
    for object_id in replies {
        let object: Object = fetch_object(&agent, &object_id).await?;
        log::info!("fetched object {}", object.id);
        match get_post_by_object_id(
            db_client,
            &instance.url(),
            &object.id,
        ).await {
            Ok(_) => continue,
            Err(DatabaseError::NotFound(_)) => {
                // Import post
                handle_note(
                    db_client,
                    &instance,
                    &storage,
                    object,
                    &HashMap::new(),
                ).await?;
            },
            Err(other_error) => return Err(other_error.into()),
        };
    };
    Ok(())
}


#[cfg(test)]
mod tests {
    use crate::webfinger::types::Link;
    use super::*;

    #[test]
    fn test_jrd_find_actor_id() {
        let actor_id = "https://social.example/users/test";
        let link_actor = Link {
            rel: "self".to_string(),
            media_type: Some("application/activity+json".to_string()),
            href: Some(actor_id.to_string()),
            properties: Default::default(),
        };
        let jrd = JsonResourceDescriptor {
            subject: "acct:test@social.example".to_string(),
            links: vec![link_actor],
        };
        assert_eq!(jrd.find_actor_id().unwrap(), actor_id);
    }
}
