use std::collections::HashMap;

use chrono::{Duration, Utc};
use serde::{
    Deserialize,
    de::DeserializeOwned,
};
use serde_json::{Value as JsonValue};

use apx_core::{
    crypto_eddsa::ed25519_secret_key_from_multikey,
    crypto_rsa::rsa_secret_key_from_multikey,
    http_url::HttpUrl,
    urls::guess_protocol,
};
use apx_sdk::{
    addresses::WebfingerAddress,
    agent::FederationAgent,
    authentication::verify_portable_object,
    deserialization::{deserialize_into_object_id_opt, get_object_id},
    fetch::{
        fetch_json,
        fetch_object,
        FetchError,
        FetchObjectOptions,
    },
    jrd::JsonResourceDescriptor,
    url::{parse_url, Url},
};
use mitra_config::{Config, Instance, MediaLimits};
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
    profiles::types::{DbActor, DbActorProfile},
    users::queries::{
        check_local_username_unique,
        create_portable_user,
        get_user_by_name,
        is_valid_invite_code,
    },
    users::types::{PortableUser, PortableUserData, User},
};
use mitra_services::media::MediaStorage;
use mitra_validators::{
    errors::ValidationError,
    users::validate_portable_user_data,
};

use crate::{
    actors::handlers::{
        create_remote_profile,
        update_remote_profile,
        Actor,
    },
    agent::build_federation_agent,
    errors::HandlerError,
    filter::FederationFilter,
    handlers::{
        activity::handle_activity,
        note::{
            create_remote_post,
            update_remote_post,
            AttributedObjectJson,
        },
    },
    identifiers::{
        canonicalize_id,
        parse_local_actor_id,
        parse_local_object_id,
    },
    ownership::verify_object_owner,
    vocabulary::GROUP,
};

pub struct ApClient {
    pub instance: Instance,
    pub filter: FederationFilter,
    pub media_limits: MediaLimits,
    pub media_storage: MediaStorage,
    pub as_user: Option<User>,
}

impl ApClient {
    pub async fn new(
        config: &Config,
        db_client: &impl DatabaseClient,
    ) -> Result<Self, DatabaseError> {
        let ap_client = Self {
            instance: config.instance(),
            filter: FederationFilter::init(config, db_client).await?,
            media_limits: config.limits.media.clone(),
            media_storage: MediaStorage::new(config),
            as_user: None,
        };
        Ok(ap_client)
    }
}

// Gateway pool for resolving 'ap' URLs
pub struct FetcherContext {
    gateways: Vec<String>,
}

impl FetcherContext {
    fn remove_gateway(&mut self, gateway_url: &str) -> () {
        self.gateways.retain(|url| url != gateway_url);
    }
}

impl From<Vec<String>> for FetcherContext {
    fn from(gateways: Vec<String>) -> Self {
        Self { gateways }
    }
}

impl From<&DbActor> for FetcherContext {
    fn from(db_actor: &DbActor) -> Self {
        Self { gateways: db_actor.gateways.clone() }
    }
}

impl FetcherContext {
    fn prepare_object_id(&mut self, object_id: &str) -> Result<String, FetchError> {
        let (canonical_object_id, maybe_gateway) = parse_url(object_id)
            .map_err(|_| FetchError::UrlError)?;
        if let Some(gateway) = maybe_gateway {
            if !self.gateways.contains(&gateway) {
                self.gateways.insert(0, gateway);
            };
        };
        // TODO: FEP-EF61: use random gateway
        let maybe_gateway = self.gateways.first()
            .map(|gateway| gateway.as_str());
        // TODO: FEP-EF61: remove Url::to_http_url
        let http_url = canonical_object_id
            .to_http_url(maybe_gateway)
            .ok_or(FetchError::NoGateway)?;
        Ok(http_url)
    }
}

pub async fn fetch_any_object_with_context<T: DeserializeOwned>(
    agent: &FederationAgent,
    context: &mut FetcherContext,
    object_id: &str,
    options: FetchObjectOptions,
) -> Result<T, FetchError> {
    let http_url = context.prepare_object_id(object_id)?;
    let object_json = fetch_object(
        agent,
        &http_url,
        options,
    ).await?;
    // TODO: convert into HandlerError::ValidationError
    let object: T = serde_json::from_value(object_json)?;
    Ok(object)
}

pub async fn fetch_any_object<T: DeserializeOwned>(
    agent: &FederationAgent,
    object_id: &str,
) -> Result<T, FetchError> {
    let mut context = FetcherContext { gateways: vec![] };
    fetch_any_object_with_context(
        agent,
        &mut context,
        object_id,
        FetchObjectOptions::default(),
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

// Actor must be authenticated
pub async fn import_profile(
    ap_client: &ApClient,
    db_client: &mut impl DatabaseClient,
    actor: JsonValue,
) -> Result<DbActorProfile, HandlerError> {
    let actor: Actor = serde_json::from_value(actor)?;
    if actor.is_local(&ap_client.instance.hostname())? {
        return Err(HandlerError::LocalObject);
    };
    let canonical_actor_id = canonicalize_id(actor.id())?;
    let profile = match get_remote_profile_by_actor_id(
        db_client,
        &canonical_actor_id.to_string(),
    ).await {
        Ok(profile) => {
            log::info!("re-fetched actor {}", actor.id());
            let profile_updated = update_remote_profile(
                ap_client,
                db_client,
                profile,
                actor,
            ).await?;
            profile_updated
        },
        Err(DatabaseError::NotFound(_)) => {
            log::info!("fetched actor {}", actor.id());
            let profile = create_remote_profile(
                ap_client,
                db_client,
                actor,
            ).await?;
            profile
        },
        Err(other_error) => return Err(other_error.into()),
    };
    Ok(profile)
}

async fn refresh_remote_profile(
    ap_client: &ApClient,
    db_client: &mut impl DatabaseClient,
    profile: DbActorProfile,
    force: bool,
) -> Result<DbActorProfile, HandlerError> {
    let agent = build_federation_agent(&ap_client.instance, None);
    let profile = if force ||
        profile.updated_at < Utc::now() - Duration::days(1)
    {
        if profile.has_account() {
            // Local nomadic accounts should not be refreshed
            return Ok(profile);
        };
        // Try to re-fetch actor profile
        let actor_data = profile.expect_actor_data();
        let mut context = FetcherContext::from(actor_data);
        // Don't re-fetch from local gateway
        context.remove_gateway(&ap_client.instance.url());
        match fetch_any_object_with_context::<Actor>(
            &agent,
            &mut context,
            &actor_data.id,
            FetchObjectOptions::default(),
        ).await {
            Ok(actor) => {
                if canonicalize_id(actor.id())?.to_string() != actor_data.id {
                    log::warn!(
                        "ignoring actor ID change: {}",
                        actor_data.id,
                    );
                    return Ok(profile);
                };
                log::info!("re-fetched actor {}", actor_data.id);
                let profile_updated = update_remote_profile(
                    ap_client,
                    db_client,
                    profile,
                    actor,
                ).await?;
                profile_updated
            },
            Err(error) => {
                // Ignore error and return stored profile
                log::warn!(
                    "failed to re-fetch {} ({})",
                    actor_data.id,
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
    // - LocalObject: local URL
    // - FetchError: fetcher errors
    // - ValidationError: invalid actor key
    // - DatabaseError(DatabaseError::NotFound(_)): local actor not found
    // - DatabaseError: other database errors
    // - StorageError: filesystem errors
    // N/A:
    // - ServiceError, AuthError, UnsolicitedMessage
    pub async fn resolve(
        &self,
        ap_client: &ApClient,
        db_client: &mut impl DatabaseClient,
        actor_id: &str,
    ) -> Result<DbActorProfile, HandlerError> {
        let canonical_actor_id = canonicalize_id(actor_id)?;
        if canonical_actor_id.authority() == ap_client.instance.hostname() {
            // Local ID
            if self.only_remote {
                return Err(HandlerError::LocalObject);
            };
            let username = parse_local_actor_id(&ap_client.instance.url(), actor_id)?;
            let user = get_user_by_name(db_client, &username).await?;
            return Ok(user.profile);
        };
        // Remote ID
        let profile = match get_remote_profile_by_actor_id(
            db_client,
            &canonical_actor_id.to_string(),
        ).await {
            Ok(profile) => {
                refresh_remote_profile(
                    ap_client,
                    db_client,
                    profile,
                    self.force_refetch,
                ).await?
            },
            Err(DatabaseError::NotFound(_)) => {
                let agent = build_federation_agent(&ap_client.instance, None);
                let actor: JsonValue = fetch_any_object(&agent, actor_id).await?;
                import_profile(ap_client, db_client, actor).await?
            },
            Err(other_error) => return Err(other_error.into()),
        };
        Ok(profile)
    }
}

// Return true if error is not internal
pub fn is_actor_importer_error(error: &HandlerError) -> bool {
    matches!(
        error,
        HandlerError::FetchError(_) |
            HandlerError::ValidationError(_) |
            HandlerError::DatabaseError(DatabaseError::NotFound(_)))
}

pub(crate) async fn perform_webfinger_query(
    agent: &FederationAgent,
    webfinger_address: &WebfingerAddress,
) -> Result<String, HandlerError> {
    let webfinger_resource = webfinger_address.to_acct_uri();
    let webfinger_url = format!(
        "{}://{}/.well-known/webfinger",
        guess_protocol(webfinger_address.hostname()),
        webfinger_address.hostname(),
    );
    let jrd_value = fetch_json(
        agent,
        &webfinger_url,
        &[("resource", &webfinger_resource)],
    ).await?;
    let jrd: JsonResourceDescriptor = serde_json::from_value(jrd_value)?;
    // Prefer Group actor if webfinger results are ambiguous
    let actor_id = jrd.find_actor_id(GROUP)
        .ok_or(ValidationError("actor ID is not found in JRD"))?;
    Ok(actor_id)
}

pub async fn import_profile_by_webfinger_address(
    ap_client: &ApClient,
    db_client: &mut impl DatabaseClient,
    webfinger_address: &WebfingerAddress,
) -> Result<DbActorProfile, HandlerError> {
    if webfinger_address.hostname() == ap_client.instance.hostname() {
        return Err(HandlerError::LocalObject);
    };
    let agent = build_federation_agent(&ap_client.instance, None);
    let actor_id = perform_webfinger_query(&agent, webfinger_address).await?;
    let actor: JsonValue = fetch_any_object(&agent, &actor_id).await?;
    import_profile(ap_client, db_client, actor).await
}

// Works with local profiles
pub async fn get_or_import_profile_by_webfinger_address(
    ap_client: &ApClient,
    db_client: &mut impl DatabaseClient,
    webfinger_address: &WebfingerAddress,
) -> Result<DbActorProfile, HandlerError> {
    let instance = &ap_client.instance;
    let acct = webfinger_address.acct(&instance.hostname());
    let profile = match get_profile_by_acct(
        db_client,
        &acct,
    ).await {
        Ok(profile) => {
            if webfinger_address.hostname() == instance.hostname() {
                profile
            } else {
                refresh_remote_profile(
                    ap_client,
                    db_client,
                    profile,
                    false,
                ).await?
            }
        },
        Err(db_error @ DatabaseError::NotFound(_)) => {
            if webfinger_address.hostname() == instance.hostname() {
                return Err(db_error.into());
            };
            import_profile_by_webfinger_address(
                ap_client,
                db_client,
                webfinger_address,
            ).await?
        },
        Err(other_error) => return Err(other_error.into()),
    };
    Ok(profile)
}

pub async fn get_post_by_object_id(
    db_client: &impl DatabaseClient,
    instance_url: &str,
    object_id: &Url,
) -> Result<Post, DatabaseError> {
    let object_id = object_id.to_string();
    match parse_local_object_id(instance_url, &object_id) {
        Ok(post_id) => {
            // Local post
            let post = get_local_post_by_id(db_client, post_id).await?;
            Ok(post)
        },
        Err(_) => {
            // Remote post
            let post = get_remote_post_by_object_id(db_client, &object_id).await?;
            Ok(post)
        },
    }
}

const RECURSION_DEPTH_MAX: usize = 50;

pub async fn import_post(
    ap_client: &ApClient,
    db_client: &mut impl DatabaseClient,
    object_id: String,
    object_received: Option<AttributedObjectJson>,
) -> Result<Post, HandlerError> {
    let instance = &ap_client.instance;
    let agent = build_federation_agent(instance, None);

    let mut queue = vec![object_id]; // LIFO queue
    let mut fetch_count = 0;
    let mut maybe_object = object_received;
    let mut objects: Vec<AttributedObjectJson> = vec![];
    let mut redirects: HashMap<String, String> = HashMap::new();
    let mut posts = vec![];

    // Fetch ancestors by going through inReplyTo references
    // TODO: fetch replies too
    #[allow(clippy::while_let_loop)]
    loop {
        let object_id = match queue.pop() {
            Some(object_id) => {
                if objects.iter().any(|object| object.id() == object_id) {
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
                    get_local_post_by_id(db_client, post_id).await?;
                    continue;
                };
                let canonical_object_id = canonicalize_id(&object_id)?;
                match get_remote_post_by_object_id(
                    db_client,
                    &canonical_object_id.to_string(),
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
                let object: AttributedObjectJson =
                    fetch_any_object(&agent, &object_id).await?;
                verify_object_owner(&object.value)?;
                log::info!("fetched object {}", object.id());
                fetch_count +=  1;
                object
            },
        };
        if object.id() != object_id {
            // ID of fetched object doesn't match requested ID
            if !objects.is_empty() {
                log::warn!("invalid reference: {object_id}");
            };
            // Add IDs to the map of redirects
            redirects.insert(object_id, object.id().to_owned());
            queue.push(object.id().to_owned());
            // Don't re-fetch object on the next iteration
            maybe_object = Some(object);
            continue;
        };
        if let Some(object_id) = object.in_reply_to() {
            // Fetch parent object on next iteration
            queue.push(object_id.to_owned());
        };
        for object_id in object.links() {
            // Fetch linked objects after fetching current thread
            queue.insert(0, object_id);
        };
        maybe_object = None;
        objects.push(object);
    };
    let initial_object_id = canonicalize_id(objects[0].id())?;

    // Objects are ordered according to their place in reply tree,
    // starting with the root
    objects.reverse();
    for object in objects {
        let post = create_remote_post(
            ap_client,
            db_client,
            object,
            &redirects,
        ).await?;
        posts.push(post);
    };

    let initial_post = posts.into_iter()
        .find(|post| post.object_id.as_ref() == Some(&initial_object_id.to_string()))
        .expect("requested post should be among fetched objects");
    Ok(initial_post)
}

// Object must be authenticated
pub async fn import_object(
    ap_client: &ApClient,
    db_client: &mut impl DatabaseClient,
    object: JsonValue,
) -> Result<(), HandlerError> {
    let object: AttributedObjectJson = serde_json::from_value(object)?;
    let canonical_object_id = canonicalize_id(object.id())?;
    match get_remote_post_by_object_id(
        db_client,
        &canonical_object_id.to_string(),
    ).await {
        Ok(post) => {
            update_remote_post(ap_client, db_client, post, &object).await?;
            Ok(())
        },
        Err(DatabaseError::NotFound(_)) => {
            import_post(
                ap_client,
                db_client,
                object.id().to_owned(),
                Some(object),
            ).await?;
            Ok(())
        },
        Err(other_error) => Err(other_error.into())
    }
}

// Activity must be authenticated
pub async fn import_activity(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    activity: JsonValue,
) -> Result<(), HandlerError> {
    handle_activity(
        config,
        db_client,
        &activity,
        true, // is authenticated
        true, // activity is being pulled (not a spam)
    ).await?;
    Ok(())
}

async fn fetch_collection(
    agent: &FederationAgent,
    collection_id: &str,
    limit: usize,
) -> Result<Vec<JsonValue>, HandlerError> {
    // https://www.w3.org/TR/activitystreams-core/#collections
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Collection {
        id: Url,
        first: Option<JsonValue>, // page can be embedded
        #[serde(default)]
        items: Vec<JsonValue>,
        #[serde(default)]
        ordered_items: Vec<JsonValue>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct CollectionPage {
        id: Url,
        next: Option<String>,
        #[serde(default)]
        items: Vec<JsonValue>,
        #[serde(default)]
        ordered_items: Vec<JsonValue>,
    }

    let collection: Collection =
        fetch_any_object(agent, collection_id).await?;
    log::info!("fetched collection: {collection_id}");
    let mut items = [collection.items, collection.ordered_items].concat();

    let mut page_count = 0;
    if let Some(first_page_value) = collection.first {
        // Mastodon replies collection:
        // - First page contains self-replies
        // - Next page contains replies from others
        let mut maybe_page_id = first_page_value.as_str()
            .map(|page_id| page_id.to_string());
        while items.len() < limit && page_count < 3 {
            let page = match maybe_page_id {
                Some(page_id) => {
                    let page: CollectionPage =
                        fetch_any_object(agent, &page_id).await?;
                    log::info!(
                        "fetched collection page #{}: {}",
                        page_count + 1,
                        page_id,
                    );
                    page
                },
                None if page_count == 0 => {
                    let page: CollectionPage =
                        serde_json::from_value(first_page_value.clone())?;
                    log::info!("first collection page is embedded");
                    page
                },
                None => break,
            };
            if page.id.origin() != collection.id.origin() {
                let error =
                    ValidationError("collection page has different origin");
                return Err(error.into());
            };
            items.extend(page.items);
            items.extend(page.ordered_items);
            page_count += 1;
            maybe_page_id = page.next;
        };
    };

    let mut authenticated = vec![];
    for item in items.into_iter().take(limit) {
        let item_id = get_object_id(&item)
            .map(|id| HttpUrl::parse(&id))
            .map_err(|_| ValidationError("invalid object ID"))?
            .map_err(|_| ValidationError("invalid object ID"))?;
        match item {
            JsonValue::String(_) => (),
            _ => {
                if item_id.origin() == collection.id.origin() {
                    // Can be trusted
                    authenticated.push(item);
                    continue
                };
            },
        };
        match fetch_any_object(agent, item_id.as_str()).await {
            Ok(item) => authenticated.push(item),
            Err(error) => {
                log::warn!("failed to fetch item ({error}): {item_id}");
                continue;
            },
        };
    };
    Ok(authenticated)
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
    let mut context = FetcherContext::from(actor_data);
    let outbox_url = context.prepare_object_id(&actor_data.outbox)?;
    let activities =
        fetch_collection(&agent, &outbox_url, limit).await?;
    log::info!("fetched {} activities", activities.len());
    // Outbox has reverse chronological order
    let activities = activities.into_iter().rev();
    for activity in activities {
        handle_activity(
            config,
            db_client,
            &activity,
            true, // is authenticated
            true, // activity is being pulled (not a spam)
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
    use_context: bool,
    use_container: bool,
    limit: usize,
) -> Result<(), HandlerError> {
    #[derive(Deserialize)]
    struct ConversationItem {
        #[serde(default, deserialize_with = "deserialize_into_object_id_opt")]
        context: Option<String>,
        #[serde(default, deserialize_with = "deserialize_into_object_id_opt")]
        replies: Option<String>,
    }

    let ap_client = ApClient::new(config, db_client).await?;
    let instance = config.instance();
    let agent = build_federation_agent(&instance, None);
    let object: ConversationItem = fetch_any_object(&agent, object_id).await?;
    if use_container {
        if let Some(ref collection_id) = object.context {
            // Converstion container
            let activities =
                fetch_collection(&agent, collection_id, limit).await?;
            log::info!("fetched {} activities", activities.len());
            for activity in activities {
                handle_activity(
                    config,
                    db_client,
                    &activity,
                    true, // is authenticated
                    true, // activity is being pulled (not a spam)
                ).await.unwrap_or_else(|error| {
                    log::warn!(
                        "failed to process activity ({}): {}",
                        error,
                        activity,
                    );
                });
            };
            return Ok(());
        } else {
            return Err(ValidationError("object doesn't have `context`").into());
        };
    };
    let maybe_collection_id = if use_context {
        object.context
    } else {
        object.replies
    };
    let collection_items = if let Some(collection_id) = maybe_collection_id {
        fetch_collection(&agent, &collection_id, limit).await?
    } else {
        vec![] // no context, no replies
    };
    log::info!("found {} items in conversation", collection_items.len());
    for item in collection_items {
        let object: AttributedObjectJson = serde_json::from_value(item)
            .map_err(|_| ValidationError("invalid conversation item"))?;
        let object_id = object.id().to_owned();
        import_post(
            &ap_client,
            db_client,
            object_id.clone(),
            Some(object),
        ).await.map_err(|error| {
            log::warn!(
                "failed to import post ({}): {}",
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
    let rsa_secret_key = rsa_secret_key_from_multikey(rsa_secret_key_multibase)
        .map_err(|_| ValidationError("invalid RSA key"))?;
    let ed25519_secret_key = ed25519_secret_key_from_multikey(ed25519_secret_key_multibase)
        .map_err(|_| ValidationError("invalid Ed25519 key"))?;
    verify_portable_object(&actor_json)
        .map_err(|error| {
            log::warn!("{error}");
            ValidationError("invalid portable actor")
        })?;
    let actor: Actor = serde_json::from_value(actor_json.clone())?;
    check_local_username_unique(
        db_client,
        actor.preferred_username(),
    ).await?;
    if !is_valid_invite_code(db_client, invite_code).await? {
        return Err(ValidationError("invalid invite code").into());
    };
    // Create or update profile
    let ap_client = ApClient::new(config, db_client).await?;
    let canonical_actor_id = canonicalize_id(actor.id())?;
    let profile = match get_remote_profile_by_actor_id(
        db_client,
        &canonical_actor_id.to_string(),
    ).await {
        Ok(profile) => {
            let profile_updated = update_remote_profile(
                &ap_client,
                db_client,
                profile,
                actor,
            ).await?;
            profile_updated
        },
        Err(DatabaseError::NotFound(_)) => {
            let profile = create_remote_profile(
                &ap_client,
                db_client,
                actor,
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
    validate_portable_user_data(&user_data, &profile)?;
    let user = create_portable_user(db_client, user_data).await?;
    create_signup_notifications(db_client, user.id).await?;
    Ok(user)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fetcher_context() {
        let gateways = vec![];
        let actor_id = "https://social.example/.well-known/apgateway/did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor";
        let mut context = FetcherContext::from(gateways);
        let http_url = context.prepare_object_id(actor_id).unwrap();
        assert_eq!(http_url, actor_id);
        let object_id = "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/objects/1";
        let http_url = context.prepare_object_id(object_id).unwrap();
        assert_eq!(
            http_url,
            "https://social.example/.well-known/apgateway/did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/objects/1",
        );
    }

    #[test]
    fn test_actor_id_resolver_default() {
        let resolver = ActorIdResolver::default();
        assert_eq!(resolver.only_remote, false);
        assert_eq!(resolver.force_refetch, false);
        let resolver = resolver.only_remote();
        assert_eq!(resolver.only_remote, true);
    }
}
