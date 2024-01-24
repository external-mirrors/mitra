use std::collections::HashMap;

use chrono::{Duration, Utc};
use serde::Deserialize;
use serde_json::{Value as JsonValue};

use mitra_config::{Config, Instance};
use mitra_federation::{
    addresses::ActorAddress,
    agent::FederationAgent,
    deserialization::get_object_id,
    fetch::{
        fetch_json,
        fetch_object,
        FetchError,
    },
    jrd::JsonResourceDescriptor,
};
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
    users::queries::get_user_by_name,
};
use mitra_services::media::MediaStorage;
use mitra_utils::urls::guess_protocol;
use mitra_validators::errors::ValidationError;

use crate::activitypub::{
    actors::helpers::{create_remote_profile, update_remote_profile},
    actors::types::Actor,
    agent::build_federation_agent,
    handlers::create::{get_object_links, handle_note, AttributedObject},
    identifiers::{parse_local_actor_id, parse_local_object_id},
    receiver::{handle_activity, HandlerError},
    vocabulary::GROUP,
};

pub async fn get_profile_by_actor_id(
    db_client: &impl DatabaseClient,
    instance_url: &str,
    actor_id: &str,
) -> Result<DbActorProfile, DatabaseError> {
    match parse_local_actor_id(instance_url, actor_id) {
        Ok(username) => {
            // Local actor
            let user = get_user_by_name(db_client, &username).await?;
            Ok(user.profile)
        },
        Err(_) => {
            // Remote actor
            get_profile_by_remote_actor_id(db_client, actor_id).await
        },
    }
}

async fn import_profile(
    db_client: &mut impl DatabaseClient,
    instance: &Instance,
    storage: &MediaStorage,
    actor_id: &str,
) -> Result<DbActorProfile, HandlerError> {
    let agent = build_federation_agent(instance, None);
    let actor: Actor = fetch_object(&agent, actor_id).await?;
    let actor_address = actor.address()?;
    if actor_address.hostname == instance.hostname() {
        return Err(HandlerError::LocalObject);
    };
    let acct = actor_address.acct(&instance.hostname());
    // 'acct' is the primary identifier
    let profile = match get_profile_by_acct(db_client, &acct).await {
        Ok(profile) => {
            // WARNING: Possible actor ID change
            log::info!("re-fetched profile {}", profile.acct);
            let profile_updated = update_remote_profile(
                &agent,
                db_client,
                storage,
                profile,
                actor,
                false,
            ).await?;
            profile_updated
        },
        Err(DatabaseError::NotFound(_)) => {
            log::info!("fetched profile {}", acct);
            let profile = create_remote_profile(
                &agent,
                db_client,
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
    force: bool,
    update_username: bool,
) -> Result<DbActorProfile, HandlerError> {
    let agent = build_federation_agent(instance, None);
    let actor_id = &profile.actor_json.as_ref()
        .expect("actor data should be present")
        .id;
    let profile = if force ||
        profile.updated_at < Utc::now() - Duration::days(1)
    {
        // Try to re-fetch actor profile
        match fetch_object::<Actor>(&agent, actor_id).await {
            Ok(actor) => {
                log::info!("re-fetched profile {}", profile.acct);
                let profile_updated = update_remote_profile(
                    &agent,
                    db_client,
                    storage,
                    profile,
                    actor,
                    update_username,
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

#[derive(Default)]
pub struct ActorIdResolver {
    only_remote: bool,
    force_refetch: bool,
    update_username: bool,
}

impl ActorIdResolver {
    pub fn only_remote(mut self) -> Self {
        self.only_remote = true;
        self
    }

    pub fn force_refetch(mut self) -> Self {
        self.force_refetch = true;
        self
    }

    pub fn update_username(mut self) -> Self {
        assert!(
            self.force_refetch,
            "'update_username' can only be used with 'force_refetch'",
        );
        self.update_username = true;
        self
    }

    // Possible errors:
    // - LocalObject: local URL, but not an actor ID
    // - FetchError: fetcher errors
    // - ValidationError: invalid actor key
    // - DatabaseError(DatabaseError::NotFound(_)): local actor not found
    // - DatabaseError: other database errors
    // - StorageError: filesystem errors
    // N/A:
    // - ServiceError, AuthError, UnsolicitedMessage
    pub async fn resolve(
        &self,
        db_client: &mut impl DatabaseClient,
        instance: &Instance,
        storage: &MediaStorage,
        actor_id: &str,
    ) -> Result<DbActorProfile, HandlerError> {
        if !self.only_remote {
            if let Ok(username) = parse_local_actor_id(&instance.url(), actor_id) {
                // Local ID
                let user = get_user_by_name(db_client, &username).await?;
                return Ok(user.profile);
            };
        };
        // Remote ID
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
                    self.force_refetch,
                    self.update_username,
                ).await?
            },
            Err(DatabaseError::NotFound(_)) => {
                import_profile(db_client, instance, storage, actor_id).await?
            },
            Err(other_error) => return Err(other_error.into()),
        };
        Ok(profile)
    }
}

async fn perform_webfinger_query(
    agent: &FederationAgent,
    actor_address: &ActorAddress,
) -> Result<String, HandlerError> {
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
    // Prefer Group actor if webfinger results are ambiguous
    let actor_id = jrd.find_actor_id(GROUP)
        .ok_or(ValidationError("actor ID is not found in JRD"))?;
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
    let agent = build_federation_agent(instance, None);
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
                    false,
                    false,
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
    object_received: Option<AttributedObject>,
) -> Result<Post, HandlerError> {
    let agent = build_federation_agent(instance, None);

    let mut queue = vec![object_id]; // LIFO queue
    let mut fetch_count = 0;
    let mut maybe_object = object_received;
    let mut objects: Vec<AttributedObject> = vec![];
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
                let object: AttributedObject =
                    fetch_object(&agent, &object_id).await?;
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

async fn fetch_collection(
    agent: &FederationAgent,
    collection_url: &str,
    limit: usize,
) -> Result<Vec<JsonValue>, FetchError> {
    // https://www.w3.org/TR/activitystreams-core/#collections
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Collection {
        first: Option<JsonValue>, // page can be embedded
        #[serde(default)]
        items: Vec<JsonValue>,
        #[serde(default)]
        ordered_items: Vec<JsonValue>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct CollectionPage {
        next: Option<String>,
        #[serde(default)]
        items: Vec<JsonValue>,
        #[serde(default)]
        ordered_items: Vec<JsonValue>,
    }

    let collection: Collection =
        fetch_object(agent, collection_url).await?;
    let mut items = [collection.items, collection.ordered_items].concat();
    if let Some(first_page_value) = collection.first {
        // Mastodon replies collection:
        // - First page contains self-replies
        // - Next page contains replies from others
        let first_page: CollectionPage = match first_page_value {
            JsonValue::String(first_page_url) => {
                fetch_object(agent, &first_page_url).await?
            },
            _ => serde_json::from_value(first_page_value)?,
        };
        items.extend(first_page.items);
        items.extend(first_page.ordered_items);
        if let Some(next_page_url) = first_page.next {
            let next_page: CollectionPage =
                fetch_object(agent, &next_page_url).await?;
            items.extend(next_page.items);
            items.extend(next_page.ordered_items);
        };
    };
    let items = items.into_iter()
        .take(limit)
        .collect();
    Ok(items)
}

pub async fn import_from_outbox(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    actor_id: &str,
    limit: usize,
) -> Result<(), HandlerError> {
    let instance = config.instance();
    let agent = build_federation_agent(&instance, None);
    let actor: Actor = fetch_object(&agent, actor_id).await?;
    let activities =
        fetch_collection(&agent, &actor.outbox, limit).await?;
    log::info!("fetched {} activities", activities.len());
    // Outbox has reverse chronological order
    let activities = activities.into_iter().rev();
    for activity in activities {
        let activity_actor = get_object_id(&activity["actor"])
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
    let agent = build_federation_agent(&instance, None);
    let storage = MediaStorage::from(config);
    let object: JsonValue = fetch_object(&agent, object_id).await?;
    let collection_items = match &object["replies"] {
        JsonValue::Null => vec![], // no replies
        value => {
            let collection_id = get_object_id(value)
                .map_err(|_| ValidationError("invalid 'replies' value"))?;
            fetch_collection(&agent, &collection_id, limit).await?
        },
    };
    log::info!("found {} replies", collection_items.len());
    for item in collection_items {
        let object_id = get_object_id(&item)
            .map_err(|_| ValidationError("invalid reply"))?;
        let object: AttributedObject =
            fetch_object(&agent, &object_id).await?;
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
    use super::*;

    #[test]
    fn test_actor_id_resolver_default() {
        let resolver = ActorIdResolver::default();
        assert_eq!(resolver.only_remote, false);
        assert_eq!(resolver.force_refetch, false);
        assert_eq!(resolver.update_username, false);
        let resolver = resolver.only_remote();
        assert_eq!(resolver.only_remote, true);
        assert_eq!(resolver.update_username, false);
    }

    #[test]
    #[should_panic(expected = "'update_username' can only be used with 'force_refetch'")]
    fn test_actor_id_resolver_check_update_username() {
        ActorIdResolver::default().update_username();
    }
}
