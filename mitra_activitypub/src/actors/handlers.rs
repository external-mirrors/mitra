use serde::{
    Deserialize,
    Deserializer,
    de::{Error as DeserializerError},
};
use serde_json::{Value as JsonValue};
use uuid::Uuid;

use apx_core::{
    http_url::Hostname,
    urls::get_hostname,
};
use apx_sdk::{
    addresses::WebfingerAddress,
    agent::FederationAgent,
    deserialization::{
        deserialize_object_array,
        deserialize_string_array,
        parse_into_array,
        parse_into_href_array,
        parse_into_id_array,
    },
    fetch::fetch_file,
    url::{is_same_origin, Url},
};
use mitra_config::MediaLimits;
use mitra_models::{
    activitypub::queries::save_actor,
    database::DatabaseClient,
    filter_rules::types::FilterAction,
    media::types::MediaInfo,
    profiles::queries::{create_profile, update_profile},
    profiles::types::{
        DbActor,
        DbActorKey,
        DbActorProfile,
        ExtraField,
        IdentityProof,
        MentionPolicy,
        PaymentOption,
        ProfileImage,
        ProfileCreateData,
        ProfileUpdateData,
        WebfingerHostname,
    },
};
use mitra_services::media::{MediaStorage, MediaStorageError};
use mitra_validators::{
    activitypub::validate_object_id,
    errors::ValidationError,
    media::validate_media_url,
    posts::EMOJI_LIMIT,
    profiles::{
        allowed_profile_image_media_types,
        clean_profile_create_data,
        clean_profile_update_data,
        validate_actor_data,
        ALIAS_LIMIT,
    },
};

use crate::{
    agent::build_federation_agent,
    errors::HandlerError,
    filter::get_moderation_domain,
    handlers::{
        emoji::handle_emoji,
        proposal::{parse_proposal, Proposal},
    },
    identifiers::canonicalize_id,
    importers::{fetch_any_object, perform_webfinger_query, ApClient},
    vocabulary::{
        APPLICATION,
        EMOJI,
        HASHTAG,
        LINK,
        NOTE,
        PROPERTY_VALUE,
        SERVICE,
        VERIFIABLE_IDENTITY_STATEMENT,
    },
};

use super::{
    attachments::{
        parse_identity_proof_fep_c390,
        parse_link,
        parse_metadata_field,
        parse_property_value,
        LinkAttachment,
    },
    builders::ActorImage,
    keys::{Multikey, PublicKey},
};

pub struct Actor {
    inner: ValidatedActor,
    value: JsonValue,
}

impl<'de> Deserialize<'de> for Actor {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where D: Deserializer<'de>
    {
        let value = JsonValue::deserialize(deserializer)?;
        let inner: ValidatedActor = serde_json::from_value(value.clone())
            .map_err(|error| {
                log::warn!("{error}");
                DeserializerError::custom("invalid actor object")
            })?;
        Ok(Self { inner, value })
    }
}

impl Actor {
    pub fn id(&self) -> &str {
        &self.inner.id
    }

    pub fn preferred_username(&self) -> &str {
        &self.inner.preferred_username
    }

    pub fn is_local(&self, local_hostname: &str) -> Result<bool, ValidationError> {
        let canonical_actor_id = Url::parse(self.id())
            .map_err(|_| ValidationError("invalid actor ID"))?;
        Ok(canonical_actor_id.authority() == local_hostname)
    }
}

fn deserialize_image_opt<'de, D>(
    deserializer: D,
) -> Result<Option<ActorImage>, D::Error>
    where D: Deserializer<'de>
{
    let maybe_value: Option<JsonValue> = Option::deserialize(deserializer)?;
    let maybe_image = if let Some(value) = maybe_value {
        match parse_into_array::<ActorImage>(&value) {
            Ok(images) => {
                // Take first image
                images.into_iter().next()
            },
            Err(_) => {
                log::warn!("ignoring invalid actor image: {value}");
                None
            },
        }
    } else {
        None
    };
    Ok(maybe_image)
}

fn deserialize_url_opt<'de, D>(
    deserializer: D,
) -> Result<Option<String>, D::Error>
    where D: Deserializer<'de>
{
    let maybe_value: Option<JsonValue> = Option::deserialize(deserializer)?;
    let maybe_url = if let Some(value) = maybe_value {
        let urls = parse_into_href_array(&value)
            .map_err(DeserializerError::custom)?;
        // Take first url
        urls.into_iter().next()
    } else {
        None
    };
    Ok(maybe_url)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Endpoints {
    shared_inbox: Option<String>,
}

#[derive(Deserialize)]
#[cfg_attr(test, derive(Default))]
#[serde(rename_all = "camelCase")]
struct ValidatedActor {
    id: String,

    #[serde(rename = "type")]
    object_type: String,

    name: Option<String>,
    preferred_username: String,

    inbox: String,
    outbox: String,
    followers: Option<String>,
    subscribers: Option<String>,
    featured: Option<String>,
    endpoints: Option<Endpoints>,

    #[serde(default, deserialize_with = "deserialize_object_array")]
    assertion_method: Vec<Multikey>,

    public_key: PublicKey,

    #[serde(default, deserialize_with = "deserialize_image_opt")]
    icon: Option<ActorImage>,

    #[serde(default, deserialize_with = "deserialize_image_opt")]
    image: Option<ActorImage>,

    summary: Option<String>,

    also_known_as: Option<JsonValue>,

    #[serde(default, deserialize_with = "deserialize_object_array")]
    attachment: Vec<JsonValue>,

    #[serde(default)]
    manually_approves_followers: bool,

    #[serde(default, deserialize_with = "deserialize_object_array")]
    tag: Vec<JsonValue>,

    #[serde(default, deserialize_with = "deserialize_url_opt")]
    url: Option<String>,

    #[serde(default, deserialize_with = "deserialize_string_array")]
    gateways: Vec<String>,
}

impl ValidatedActor {
    fn is_automated(&self) -> bool {
        [APPLICATION, SERVICE].contains(&self.object_type.as_str())
    }

    fn to_db_actor(&self) -> Result<DbActor, ValidationError> {
        let canonical_actor_id = canonicalize_id(&self.id)?;
        let canonical_inbox = canonicalize_id(&self.inbox)?;
        let canonical_outbox = canonicalize_id(&self.outbox)?;
        let maybe_canonical_followers = self.followers.as_deref()
            .map(canonicalize_id)
            .transpose()?;
        let maybe_canonical_subscribers = self.subscribers.as_deref()
            .map(canonicalize_id)
            .transpose()?;
        let maybe_canonical_featured = self.featured.as_deref()
            .map(canonicalize_id)
            .transpose()?;
        let db_actor = DbActor {
            object_type: self.object_type.clone(),
            id: canonical_actor_id.to_string(),
            inbox: canonical_inbox.to_string(),
            shared_inbox: self.endpoints.as_ref()
                .and_then(|endpoints| endpoints.shared_inbox.clone()),
            outbox: canonical_outbox.to_string(),
            followers: maybe_canonical_followers.map(|id| id.to_string()),
            subscribers: maybe_canonical_subscribers.map(|id| id.to_string()),
            featured: maybe_canonical_featured.map(|id| id.to_string()),
            url: self.url.clone(),
            gateways: self.gateways.clone(),
            public_key: None,
        };
        Ok(db_actor)
    }
}

// Determine hostname part of 'acct' URI
async fn get_webfinger_hostname(
    agent: &FederationAgent,
    instance_hostname: &str,
    actor: &ValidatedActor,
    has_account: bool,
) -> Result<WebfingerHostname, HandlerError> {
    let canonical_actor_id = Url::parse(&actor.id)
        .map_err(|_| ValidationError("invalid actor ID"))?;
    let webfinger_hostname = match canonical_actor_id {
        Url::Http(http_url) => {
            // TODO: implement reverse webfinger lookup
            // https://swicg.github.io/activitypub-webfinger/#reverse-discovery
            let hostname = get_hostname(&http_url.to_string())
                .map_err(|_| ValidationError("invalid actor ID"))?;
            WebfingerHostname::Remote(hostname)
        },
        Url::Ap(_) => {
            if let Some(gateway) = actor.gateways.first() {
                // Primary gateway
                let hostname = get_hostname(gateway)
                    .map_err(|_| ValidationError("invalid gateway URL"))?;
                if hostname == instance_hostname {
                    // Portable actor with local account (unmanaged)
                    if has_account {
                        return Ok(WebfingerHostname::Local);
                    } else {
                        return Ok(WebfingerHostname::Unknown);
                    };
                };
                let webfinger_address = WebfingerAddress::new_unchecked(
                    &actor.preferred_username,
                    &hostname,
                );
                let actor_id = perform_webfinger_query(
                    agent,
                    &webfinger_address,
                ).await?;
                if actor_id == actor.id {
                    WebfingerHostname::Remote(hostname)
                } else {
                    return Err(ValidationError("unexpected actor ID in JRD").into());
                }
            } else {
                return Err(ValidationError("at least one gateway must be specified").into());
            }
        },
    };
    Ok(webfinger_hostname)
}

async fn fetch_actor_image(
    agent: &FederationAgent,
    is_filter_enabled: bool,
    media_limits: &MediaLimits,
    media_storage: &MediaStorage,
    actor_image: &Option<ActorImage>,
    default: Option<ProfileImage>,
) -> Result<Option<ProfileImage>, MediaStorageError> {
    let maybe_image = if let Some(actor_image) = actor_image {
        if let Err(error) = validate_media_url(&actor_image.url) {
            log::warn!("invalid actor image URL ({error}): {}", actor_image.url);
            return Ok(default);
        };
        if is_filter_enabled {
            log::warn!("actor image removed by filter: {}", actor_image.url);
            return Ok(None);
        };
        match fetch_file(
            agent,
            &actor_image.url,
            actor_image.media_type.as_deref(),
            &allowed_profile_image_media_types(&media_limits.supported_media_types()),
            media_limits.profile_image_size_limit,
        ).await {
            Ok((file_data, media_type)) => {
                let file_info = media_storage.save_file(file_data, &media_type)?;
                let image = ProfileImage::from(MediaInfo::remote(
                    file_info,
                    actor_image.url.clone(),
                ));
                Some(image)
            },
            Err(error) => {
                log::warn!("failed to fetch actor image ({error})");
                default
            },
        }
    } else {
        None
    };
    Ok(maybe_image)
}

async fn fetch_actor_images(
    agent: &FederationAgent,
    is_filter_enabled: bool,
    media_limits: &MediaLimits,
    media_storage: &MediaStorage,
    actor: &ValidatedActor,
    default_avatar: Option<ProfileImage>,
    default_banner: Option<ProfileImage>,
) -> Result<(Option<ProfileImage>, Option<ProfileImage>), MediaStorageError> {
    let maybe_avatar = fetch_actor_image(
        agent,
        is_filter_enabled,
        media_limits,
        media_storage,
        &actor.icon,
        default_avatar,
    ).await?;
    let maybe_banner = fetch_actor_image(
        agent,
        is_filter_enabled,
        media_limits,
        media_storage,
        &actor.image,
        default_banner,
    ).await?;
    Ok((maybe_avatar, maybe_banner))
}

fn parse_public_keys(
    actor: &ValidatedActor,
) -> Result<Vec<DbActorKey>, ValidationError> {
    let mut keys = vec![];
    if actor.public_key.owner != actor.id {
        return Err(ValidationError("public key is not owned by actor"));
    };
    let db_key = actor.public_key.to_db_key()?;
    keys.push(db_key);
    let verification_methods = &actor.assertion_method;
    for multikey in verification_methods {
        if multikey.controller == actor.id {
            let db_key = multikey.to_db_key()?;
            keys.push(db_key);
        } else {
            return Err(ValidationError("verification method is not owned by actor"));
        };
    };
    keys.sort_by_key(|item| item.id.clone());
    keys.dedup_by_key(|item| item.id.clone());
    if keys.is_empty() {
        return Err(ValidationError("public keys not found"));
    };
    for key in keys.iter() {
        if !is_same_origin(&key.id, &actor.id)
            .map_err(|message| ValidationError(message.0))?
        {
            // Cross-origin relationships are possible in origin-based security model,
            // but we don't allow them yet.
            // https://codeberg.org/fediverse/fep/src/branch/main/fep/fe34/fep-fe34.md
            return Err(ValidationError("actor and actor key have different origins"));
        };
    };
    Ok(keys)
}

fn parse_attachments(actor: &ValidatedActor) -> (
    Vec<IdentityProof>,
    Vec<PaymentOption>,
    Vec<String>,
    Vec<ExtraField>,
) {
    let mut identity_proofs = vec![];
    let mut payment_options = vec![];
    let mut proposals = vec![];
    let mut extra_fields = vec![];
    let mut property_values = vec![];
    let log_error = |attachment_type: &str, error| {
        log::warn!(
            "ignoring actor attachment of type {}: {}",
            attachment_type,
            error,
        );
    };
    for attachment_value in actor.attachment.iter() {
        let attachment_type =
            attachment_value["type"].as_str().unwrap_or("Unknown");
        match attachment_type {
            VERIFIABLE_IDENTITY_STATEMENT => {
                match parse_identity_proof_fep_c390(&actor.id, attachment_value) {
                    Ok(proof) => identity_proofs.push(proof),
                    Err(error) => log_error(attachment_type, error),
                };
            },
            LINK => {
                match parse_link(attachment_value) {
                    Ok(LinkAttachment::PaymentLink(payment_link)) => {
                        let option = PaymentOption::Link(payment_link);
                        payment_options.push(option);
                    },
                    Ok(LinkAttachment::Proposal(payment_link)) => {
                        // Only one proposal is allowed
                        // (uniqueness check on payment type is performed in
                        // profiles::checks::check_payment_options).
                        // The remaining proposals are treated as payment links.
                        if proposals.is_empty() {
                            proposals.push(payment_link.href);
                        } else {
                            let option = PaymentOption::Link(payment_link);
                            payment_options.push(option);
                        };
                    },
                    Ok(LinkAttachment::OtherLink(field)) => {
                        extra_fields.push(field);
                    },
                    Err(error) => log_error(attachment_type, error),
                };
            },
            PROPERTY_VALUE => {
                match parse_property_value(attachment_value) {
                    Ok(field) => property_values.push(field),
                    Err(error) => log_error(attachment_type, error),
                };
            },
            NOTE => {
                match parse_metadata_field(attachment_value) {
                    Ok(field) => extra_fields.push(field),
                    Err(error) => log_error(attachment_type, error),
                };
            },
            _ => {
                log_error(
                    attachment_type,
                    ValidationError("unsupported attachment type"),
                );
            },
        };
    };
    // Remove duplicate identity proofs
    identity_proofs.sort_by_key(|item| item.issuer.to_string());
    identity_proofs.dedup_by_key(|item| item.issuer.to_string());
    // Remove duplicate metadata fields
    // FEP-fb2a fields have higher priority
    for field in property_values {
        if extra_fields.iter().any(|item| item.name == field.name) {
            continue;
        } else {
            extra_fields.push(field);
        };
    };
    (identity_proofs, payment_options, proposals, extra_fields)
}

async fn fetch_proposals(
    agent: &FederationAgent,
    proposals: Vec<String>,
) -> Vec<PaymentOption> {
    let mut payment_options = vec![];
    for proposal_id in proposals {
        // TODO: FEP-EF61: 'ap' URLs are not supported
        let proposal: Proposal = match fetch_any_object(agent, &proposal_id).await {
            Ok(proposal) => proposal,
            Err(error) => {
                log::warn!("invalid proposal: {}", error);
                continue;
            },
        };
        log::info!("fetched proposal {}", proposal.id);
        let payment_option = match parse_proposal(proposal) {
            Ok(option) => option,
            Err(error) => {
                log::warn!("invalid proposal: {}", error);
                continue;
            },
        };
        payment_options.push(payment_option);
    };
    payment_options
}

fn parse_aliases(actor: &ValidatedActor) -> Vec<String> {
    // Aliases reported by server (not signed)
    actor.also_known_as.as_ref()
        .and_then(|value| {
            match parse_into_id_array(value) {
                Ok(array) => {
                    let mut aliases = vec![];
                    for actor_id in array {
                        if aliases.len() >= ALIAS_LIMIT {
                            log::warn!("too many aliases");
                            break;
                        };
                        if actor_id == actor.id {
                            continue;
                        };
                        if let Err(error) = validate_object_id(&actor_id) {
                            log::warn!("invalid alias ({error}): {actor_id}");
                            continue;
                        };
                        aliases.push(actor_id);
                    };
                    Some(aliases)
                },
                Err(_) => {
                    log::warn!("invalid alias list: {}", value);
                    None
                },
            }
        })
        .unwrap_or_default()
}

async fn parse_tags(
    ap_client: &ApClient,
    db_client: &mut impl DatabaseClient,
    moderation_domain: &Hostname,
    actor: &ValidatedActor,
) -> Result<Vec<Uuid>, HandlerError> {
    let mut emojis = vec![];
    for tag_value in actor.tag.clone() {
        let tag_type = tag_value["type"].as_str().unwrap_or(HASHTAG);
        if tag_type == EMOJI {
            if emojis.len() >= EMOJI_LIMIT {
                log::warn!("too many emojis");
                continue;
            };
            match handle_emoji(
                ap_client,
                db_client,
                moderation_domain,
                tag_value,
            ).await? {
                Some(emoji) => {
                    if !emojis.contains(&emoji.id) {
                        emojis.push(emoji.id);
                    };
                },
                None => continue,
            };
        } else {
            log::warn!("skipping tag of type {tag_type}");
        };
    };
    Ok(emojis)
}

pub async fn create_remote_profile(
    ap_client: &ApClient,
    db_client: &mut impl DatabaseClient,
    actor: Actor,
) -> Result<DbActorProfile, HandlerError> {
    let Actor { inner: actor, value: actor_json } = actor;
    let agent = build_federation_agent(&ap_client.instance, None);
    let webfinger_hostname = get_webfinger_hostname(
        &agent,
        &ap_client.instance.hostname(),
        &actor,
        false,
    ).await?;
    let actor_data = actor.to_db_actor()?;
    validate_actor_data(&actor_data)?;
    let moderation_domain = get_moderation_domain(&actor_data)
        .expect("actor data should be valid");
    if ap_client.filter.is_action_required(
        moderation_domain.as_str(),
        FilterAction::Reject,
    ) {
        let error_message = format!("actor rejected: {}", actor_data.id);
        return Err(HandlerError::Filtered(error_message));
    };
    let is_media_filter_enabled = ap_client.filter.is_action_required(
        moderation_domain.as_str(),
        FilterAction::RejectProfileImages,
    );
    let (maybe_avatar, maybe_banner) = fetch_actor_images(
        &agent,
        is_media_filter_enabled,
        &ap_client.media_limits,
        &ap_client.media_storage,
        &actor,
        None,
        None,
    ).await?;
    let public_keys = parse_public_keys(&actor)?;
    let (identity_proofs, mut payment_options, proposals, extra_fields) =
        parse_attachments(&actor);
    let subscription_options = fetch_proposals(
        &agent,
        proposals,
    ).await;
    payment_options.extend(subscription_options);
    let aliases = parse_aliases(&actor);
    let emojis = parse_tags(
        ap_client,
        db_client,
        &moderation_domain,
        &actor,
    ).await?;
    let mut profile_data = ProfileCreateData {
        username: actor.preferred_username.clone(),
        hostname: webfinger_hostname,
        display_name: actor.name.clone(),
        bio: actor.summary.clone(),
        avatar: maybe_avatar,
        banner: maybe_banner,
        is_automated: actor.is_automated(),
        manually_approves_followers: actor.manually_approves_followers,
        mention_policy: MentionPolicy::None,
        public_keys,
        identity_proofs,
        payment_options,
        extra_fields,
        aliases,
        emojis,
        actor_json: Some(actor_data),
    };
    clean_profile_create_data(&mut profile_data)?;
    let profile = create_profile(db_client, profile_data).await?;
    // Save actor object
    save_actor(
        db_client,
        profile.expect_remote_actor_id(),
        &actor_json,
        profile.id,
    ).await?;
    Ok(profile)
}

/// Updates remote actor's profile
pub async fn update_remote_profile(
    ap_client: &ApClient,
    db_client: &mut impl DatabaseClient,
    profile: DbActorProfile,
    actor: Actor,
) -> Result<DbActorProfile, HandlerError> {
    let Actor { inner: actor, value: actor_json } = actor;
    if actor.preferred_username != profile.username {
        log::warn!("preferred username doesn't match cached value");
    };
    let actor_data_old = profile.expect_actor_data();
    let actor_data = actor.to_db_actor()?;
    assert_eq!(actor_data_old.id, actor_data.id, "actor ID shouldn't change");
    let agent = build_federation_agent(&ap_client.instance, None);
    let webfinger_hostname = get_webfinger_hostname(
        &agent,
        &ap_client.instance.hostname(),
        &actor,
        profile.has_account(),
    ).await?;
    validate_actor_data(&actor_data)?;
    let moderation_domain = get_moderation_domain(&actor_data)
        .expect("actor data should be valid");
    let is_media_filter_enabled = ap_client.filter.is_action_required(
        moderation_domain.as_str(),
        FilterAction::RejectProfileImages,
    );
    let (maybe_avatar, maybe_banner) = fetch_actor_images(
        &agent,
        is_media_filter_enabled,
        &ap_client.media_limits,
        &ap_client.media_storage,
        &actor,
        profile.avatar,
        profile.banner,
    ).await?;
    let public_keys = parse_public_keys(&actor)?;
    let (identity_proofs, mut payment_options, proposals, extra_fields) =
        parse_attachments(&actor);
    let subscription_options = fetch_proposals(
        &agent,
        proposals,
    ).await;
    payment_options.extend(subscription_options);
    let aliases = parse_aliases(&actor);
    let emojis = parse_tags(
        ap_client,
        db_client,
        &moderation_domain,
        &actor,
    ).await?;
    let mut profile_data = ProfileUpdateData {
        username: actor.preferred_username.clone(),
        hostname: webfinger_hostname,
        display_name: actor.name.clone(),
        bio: actor.summary.clone(),
        bio_source: actor.summary.clone(),
        avatar: maybe_avatar,
        banner: maybe_banner,
        is_automated: actor.is_automated(),
        manually_approves_followers: actor.manually_approves_followers,
        mention_policy: MentionPolicy::None,
        public_keys,
        identity_proofs,
        payment_options,
        extra_fields,
        aliases,
        emojis,
        actor_json: Some(actor_data),
    };
    clean_profile_update_data(&mut profile_data)?;
    // update_profile() clears unreachable_since
    let (profile, deletion_queue) =
        update_profile(db_client, profile.id, profile_data).await?;
    // Delete orphaned images after update
    deletion_queue.into_job(db_client).await?;
    // Save actor object
    save_actor(
        db_client,
        profile.expect_remote_actor_id(),
        &actor_json,
        profile.id,
    ).await?;
    Ok(profile)
}

#[cfg(test)]
mod tests {
    use apx_core::{
        crypto_eddsa::{
            ed25519_public_key_from_secret_key,
            generate_ed25519_key,
        },
        crypto_rsa::{
            generate_weak_rsa_key,
            rsa_public_key_to_pkcs1_der,
        },
    };
    use mitra_models::profiles::types::PublicKeyType;
    use crate::actors::keys::{Multikey, PublicKey};
    use super::*;

    #[test]
    fn test_actor_is_local() {
        let actor = Actor {
            inner: ValidatedActor {
                id: "https://social.example/users/1".to_string(),
                ..Default::default()
            },
            value: Default::default()
        };
        let is_local = actor.is_local("social.example").unwrap();
        assert!(is_local);
    }

    #[test]
    fn test_actor_is_local_compatible_id() {
        let actor = Actor {
            inner: ValidatedActor {
                id: "https://gateway.example/.well-known/apgateway/did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor".to_string(),
                ..Default::default()
            },
            value: Default::default()
        };
        let is_local = actor.is_local("gateway.example").unwrap();
        assert!(!is_local);
    }

    #[test]
    fn test_deserialize_image_opt() {
        #[derive(Deserialize)]
        struct TestObject {
            #[serde(default, deserialize_with = "deserialize_image_opt")]
            image: Option<ActorImage>,
        }
        let object_value = serde_json::json!({});
        let object: TestObject = serde_json::from_value(object_value).unwrap();
        assert_eq!(object.image.is_none(), true);

        let object_value = serde_json::json!({"image": {}});
        let object: TestObject = serde_json::from_value(object_value).unwrap();
        assert_eq!(object.image.is_none(), true);

        let object_value =
            serde_json::json!({"image": "https://social.example/image.png"});
        let object: TestObject = serde_json::from_value(object_value).unwrap();
        assert_eq!(object.image.is_none(), true);
    }

    #[test]
    fn test_parse_public_keys() {
        let actor_id = "https://test.example/users/1";
        let rsa_secret_key = generate_weak_rsa_key().unwrap();
        let ed25519_secret_key = generate_ed25519_key();
        let actor_public_key =
            PublicKey::build(actor_id, &rsa_secret_key).unwrap();
        let actor_auth_key_1 =
            Multikey::build_rsa(actor_id, &rsa_secret_key).unwrap();
        let actor_auth_key_2 =
            Multikey::build_ed25519(actor_id, &ed25519_secret_key);
        let actor = ValidatedActor {
            id: actor_id.to_string(),
            public_key: actor_public_key,
            assertion_method: vec![actor_auth_key_1, actor_auth_key_2],
            ..Default::default()
        };
        let public_keys = parse_public_keys(&actor).unwrap();
        assert_eq!(public_keys.len(), 2);
        let ed25519_public_key_bytes =
            ed25519_public_key_from_secret_key(&ed25519_secret_key).to_bytes();
        assert_eq!(public_keys[0].key_type, PublicKeyType::Ed25519);
        assert_eq!(public_keys[0].key_data, ed25519_public_key_bytes);
        let rsa_public_key_der =
            rsa_public_key_to_pkcs1_der(&rsa_secret_key.into()).unwrap();
        assert_eq!(public_keys[1].key_type, PublicKeyType::RsaPkcs1);
        assert_eq!(public_keys[1].key_data, rsa_public_key_der);
    }
}
