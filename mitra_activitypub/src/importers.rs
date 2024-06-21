use std::collections::HashMap;

use chrono::{Duration, Utc};
use serde::{
    Deserialize,
    de::DeserializeOwned,
};
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
    notifications::helpers::create_signup_notifications,
    posts::helpers::get_local_post_by_id,
    posts::queries::get_remote_post_by_object_id,
    posts::types::Post,
    profiles::queries::{
        get_profile_by_acct,
        get_remote_profile_by_actor_id,
    },
    profiles::types::DbActorProfile,
    users::queries::{
        check_local_username_unique,
        create_portable_user,
        get_user_by_name,
        is_valid_invite_code,
    },
    users::types::{PortableUser, PortableUserData},
};
use mitra_services::media::MediaStorage;
use mitra_utils::{
    crypto_eddsa::ed25519_secret_key_from_multikey,
    crypto_rsa::rsa_secret_key_from_pkcs1_der,
    multibase::decode_multibase_base58btc,
    multicodec::Multicodec,
    urls::guess_protocol,
};
use mitra_validators::errors::ValidationError;

use crate::{
    actors::handlers::{
        create_remote_profile,
        update_remote_profile,
        Actor,
        ActorJson,
    },
    agent::build_federation_agent,
    authentication::{verify_portable_object, AuthenticationError},
    errors::HandlerError,
    handlers::{
        activity::handle_activity,
        create::{get_object_links, handle_note, AttributedObject},
    },
    identifiers::{parse_local_actor_id, parse_local_object_id},
    url::{canonicalize_id, parse_url, Url},
    vocabulary::GROUP,
};

// Gateway pool for resolving 'ap' URLs
pub struct FetcherContext {
    gateways: Vec<String>,
}

impl From<Vec<String>> for FetcherContext {
    fn from(gateways: Vec<String>) -> Self {
        Self { gateways }
    }
}

impl FetcherContext {
    pub fn prepare_object_id(&mut self, object_id: &str) -> Result<Url, ValidationError> {
        let (canonical_object_id, maybe_gateway) = parse_url(object_id)?;
        if let Some(gateway) = maybe_gateway {
            if !self.gateways.contains(&gateway) {
                self.gateways.insert(0, gateway);
            };
        };
        Ok(canonical_object_id)
    }
}

pub async fn fetch_any_object_with_context<T: DeserializeOwned>(
    agent: &FederationAgent,
    context: &FetcherContext,
    object_id: &Url,
) -> Result<T, FetchError> {
    // TODO: FEP-EF61: use random gateway
    let maybe_gateway = context.gateways.first()
        .map(|gateway| gateway.as_str());
    // TODO: FEP-EF61: remove Url::to_http_url
    let http_url = object_id
        .to_http_url(maybe_gateway)
        .ok_or(FetchError::NoGateway)?;
    let object_json: JsonValue = fetch_object(agent, &http_url).await?;
    match verify_portable_object(&object_json) {
        Ok(_) => (),
        Err(AuthenticationError::NotPortable) => (), // skip proof verification
        Err(_) => return Err(FetchError::InvalidProof),
    };
    let object: T = serde_json::from_value(object_json)?;
    Ok(object)
}

pub async fn fetch_any_object<T: DeserializeOwned>(
    agent: &FederationAgent,
    object_id: &str,
) -> Result<T, FetchError> {
    let mut context = FetcherContext { gateways: vec![] };
    let canonical_object_id = context.prepare_object_id(object_id)
        .map_err(|_| FetchError::UrlError)?;
    fetch_any_object_with_context(
        agent,
        &context,
        &canonical_object_id,
    ).await
}

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
            get_remote_profile_by_actor_id(db_client, actor_id).await
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
    let actor: ActorJson = fetch_any_object(&agent, actor_id).await?;
    if actor.hostname()? == instance.hostname() {
        return Err(HandlerError::LocalObject);
    };
    let profile = match get_remote_profile_by_actor_id(
        db_client,
        &actor.id,
    ).await {
        Ok(profile) => {
            log::info!("re-fetched actor {}", actor.id);
            let profile_updated = update_remote_profile(
                &agent,
                db_client,
                storage,
                profile,
                actor.value,
            ).await?;
            profile_updated
        },
        Err(DatabaseError::NotFound(_)) => {
            log::info!("fetched actor {}", actor.id);
            let profile = create_remote_profile(
                &agent,
                db_client,
                &instance.hostname(),
                storage,
                actor.value,
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
) -> Result<DbActorProfile, HandlerError> {
    let agent = build_federation_agent(instance, None);
    let actor_id = profile.expect_remote_actor_id();
    let profile = if force ||
        profile.updated_at < Utc::now() - Duration::days(1)
    {
        // Try to re-fetch actor profile
        match fetch_any_object::<ActorJson>(&agent, actor_id).await {
            Ok(actor) => {
                log::info!("re-fetched actor {}", actor.id);
                let profile_updated = update_remote_profile(
                    &agent,
                    db_client,
                    storage,
                    profile,
                    actor.value,
                ).await?;
                profile_updated
            },
            Err(error) => {
                // Ignore error and return stored profile
                log::warn!(
                    "failed to re-fetch {} ({})",
                    actor_id,
                    error,
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
        let canonical_actor_id = canonicalize_id(actor_id)?;
        let profile = match get_remote_profile_by_actor_id(
            db_client,
            &canonical_actor_id,
        ).await {
            Ok(profile) => {
                refresh_remote_profile(
                    db_client,
                    instance,
                    storage,
                    profile,
                    self.force_refetch,
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

pub fn is_actor_importer_error(error: &HandlerError) -> bool {
    matches!(
        error,
        HandlerError::FetchError(_) |
            HandlerError::ValidationError(_) |
            HandlerError::DatabaseError(DatabaseError::NotFound(_)))
}

async fn perform_webfinger_query(
    agent: &FederationAgent,
    actor_address: &ActorAddress,
) -> Result<String, HandlerError> {
    let webfinger_resource = actor_address.to_acct_uri();
    let webfinger_url = format!(
        "{}://{}/.well-known/webfinger",
        guess_protocol(actor_address.hostname()),
        actor_address.hostname(),
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
    if actor_address.hostname() == instance.hostname() {
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
            if actor_address.hostname() == instance.hostname() {
                profile
            } else {
                refresh_remote_profile(
                    db_client,
                    instance,
                    storage,
                    profile,
                    false,
                ).await?
            }
        },
        Err(db_error @ DatabaseError::NotFound(_)) => {
            if actor_address.hostname() == instance.hostname() {
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
            let post = get_remote_post_by_object_id(db_client, object_id).await?;
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
                let canonical_object_id = canonicalize_id(&object_id)?;
                match get_remote_post_by_object_id(
                    db_client,
                    &canonical_object_id,
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
                    fetch_any_object(&agent, &object_id).await?;
                log::info!("fetched object {}", object.id);
                fetch_count +=  1;
                object
            },
        };
        if object.id != object_id {
            // ID of fetched object doesn't match requested ID
            if !objects.is_empty() {
                log::warn!("invalid reference: {object_id}");
            };
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
    let initial_object_id = canonicalize_id(&objects[0].id)?;

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
        .expect("requested post should be among fetched objects");
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
        fetch_any_object(agent, collection_url).await?;
    let mut items = [collection.items, collection.ordered_items].concat();
    if let Some(first_page_value) = collection.first {
        // Mastodon replies collection:
        // - First page contains self-replies
        // - Next page contains replies from others
        let first_page: CollectionPage = match first_page_value {
            JsonValue::String(first_page_url) => {
                fetch_any_object(agent, &first_page_url).await?
            },
            _ => serde_json::from_value(first_page_value)?,
        };
        items.extend(first_page.items);
        items.extend(first_page.ordered_items);
        if let Some(next_page_url) = first_page.next {
            let next_page: CollectionPage =
                fetch_any_object(agent, &next_page_url).await?;
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
    let profile = get_remote_profile_by_actor_id(db_client, actor_id).await?;
    let actor_data = profile.expect_actor_data();
    let activities =
        fetch_collection(&agent, &actor_data.outbox, limit).await?;
    log::info!("fetched {} activities", activities.len());
    // Outbox has reverse chronological order
    let activities = activities.into_iter().rev();
    for activity in activities {
        let activity_actor = get_object_id(&activity["actor"])
            .map_err(|_| ValidationError("invalid actor property"))?;
        if activity_actor != actor_data.id {
            log::warn!("activity doesn't belong to outbox owner");
            continue;
        };
        handle_activity(
            config,
            db_client,
            &activity,
            true, // is authenticated
            true, // activity is being pulled
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
    let object: JsonValue = fetch_any_object(&agent, object_id).await?;
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
        import_post(
            db_client,
            &instance,
            &storage,
            object_id.clone(),
            None,
        ).await.map_err(|error| {
            log::warn!(
                "failed to import reply ({}): {}",
                error,
                object_id,
            );
        }).ok();
    };
    Ok(())
}

pub async fn register_portable_actor(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    actor_json: JsonValue,
    rsa_secret_key_multibase: &str,
    ed25519_secret_key_multibase: &str,
    invite_code: &str,
) -> Result<PortableUser, HandlerError> {
    let instance = config.instance();
    let agent = build_federation_agent(&instance, None);
    let storage = MediaStorage::from(config);
    let rsa_secret_key_multicode = decode_multibase_base58btc(rsa_secret_key_multibase)
        .map_err(|_| ValidationError("invalid RSA key"))?;
    let rsa_secret_key_der = Multicodec::RsaPriv.decode_exact(&rsa_secret_key_multicode)
        .map_err(|_| ValidationError("invalid RSA key"))?;
    let rsa_secret_key = rsa_secret_key_from_pkcs1_der(&rsa_secret_key_der)
        .map_err(|_| ValidationError("invalid RSA key"))?;
    let ed25519_secret_key = ed25519_secret_key_from_multikey(ed25519_secret_key_multibase)
        .map_err(|_| ValidationError("invalid Ed25519 key"))?;
    verify_portable_object(&actor_json)
        .map_err(|error| {
            log::warn!("{error}");
            ValidationError("invalid portable actor")
        })?;
    let actor: Actor = serde_json::from_value(actor_json.clone())
        .map_err(|_| ValidationError("invalid actor object"))?;
    check_local_username_unique(
        db_client,
        &actor.preferred_username,
    ).await?;
    if !is_valid_invite_code(db_client, invite_code).await? {
        return Err(ValidationError("invalid invite code").into());
    };
    // Create or update profile
    let canonical_actor_id = canonicalize_id(&actor.id)?;
    let profile = match get_remote_profile_by_actor_id(
        db_client,
        &canonical_actor_id,
    ).await {
        Ok(profile) => {
            let profile_updated = update_remote_profile(
                &agent,
                db_client,
                &storage,
                profile,
                actor_json,
            ).await?;
            profile_updated
        },
        Err(DatabaseError::NotFound(_)) => {
            // TODO: create profile and user account in a single transaction
            let profile = create_remote_profile(
                &agent,
                db_client,
                &instance.hostname(),
                &storage,
                actor_json,
            ).await?;
            profile
        },
        Err(other_error) => return Err(other_error.into()),
    };
    // Create user
    let user_data = PortableUserData {
        profile_id: profile.id,
        rsa_secret_key: rsa_secret_key,
        ed25519_secret_key: ed25519_secret_key,
        invite_code: invite_code.to_string(),
    };
    let user = create_portable_user(db_client, user_data).await?;
    create_signup_notifications(db_client, user.id).await?;
    Ok(user)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_actor_id_resolver_default() {
        let resolver = ActorIdResolver::default();
        assert_eq!(resolver.only_remote, false);
        assert_eq!(resolver.force_refetch, false);
        let resolver = resolver.only_remote();
        assert_eq!(resolver.only_remote, true);
    }
}
