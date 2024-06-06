use serde::{
    Deserialize,
    Deserializer,
    de::{Error as DeserializerError},
};
use serde_json::{Value as JsonValue};
use uuid::Uuid;

use mitra_federation::{
    agent::FederationAgent,
    deserialization::{
        deserialize_object_array,
        deserialize_string_array,
        parse_into_array,
        parse_into_href_array,
        parse_into_id_array,
    },
    fetch::fetch_file,
};
use mitra_models::{
    activitypub::queries::save_actor,
    database::DatabaseClient,
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
    },
};
use mitra_services::media::{MediaStorage, MediaStorageError};
use mitra_utils::urls::get_hostname;
use mitra_validators::{
    activitypub::validate_object_id,
    errors::ValidationError,
    posts::EMOJI_LIMIT,
    profiles::{
        allowed_profile_image_media_types,
        clean_profile_create_data,
        clean_profile_update_data,
        PROFILE_IMAGE_SIZE_MAX,
    },
};

use crate::{
    errors::HandlerError,
    handlers::{
        emoji::handle_emoji,
        proposal::{parse_proposal, Proposal},
    },
    importers::fetch_any_object,
    vocabulary::{
        EMOJI,
        HASHTAG,
        LINK,
        NOTE,
        PROPERTY_VALUE,
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

pub struct ActorJson {
    pub id: String,
    pub value: JsonValue,
}

impl<'de> Deserialize<'de> for ActorJson {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where D: Deserializer<'de>
    {
        let value = JsonValue::deserialize(deserializer)?;
        let actor_id = value["id"].as_str()
            .ok_or(DeserializerError::custom("'id' property not found"))?;
        Ok(Self {
            id: actor_id.to_string(),
            value: value,
        })
    }
}

impl ActorJson {
    pub fn hostname(&self) -> Result<String, ValidationError> {
        let hostname = get_hostname(&self.id)
            .map_err(|_| ValidationError("invalid actor ID"))?;
        Ok(hostname)
    }
}

fn deserialize_image_opt<'de, D>(
    deserializer: D,
) -> Result<Option<ActorImage>, D::Error>
    where D: Deserializer<'de>
{
    let maybe_value: Option<JsonValue> = Option::deserialize(deserializer)?;
    let maybe_image = if let Some(value) = maybe_value {
        // Some implementations use empty object instead of null
        let is_empty_object = value.as_object()
            .map(|map| map.is_empty())
            .unwrap_or(false);
        if is_empty_object {
            None
        } else {
            let images: Vec<ActorImage> = parse_into_array(&value)
                .map_err(DeserializerError::custom)?;
            // Take first image
            images.into_iter().next()
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
#[cfg_attr(test, derive(Default))]
#[serde(rename_all = "camelCase")]
pub struct Actor {
    pub id: String,

    #[serde(rename = "type")]
    object_type: String,

    name: Option<String>,
    preferred_username: String,

    inbox: String,
    outbox: String,
    followers: Option<String>,
    subscribers: Option<String>,
    featured: Option<String>,

    #[serde(default, deserialize_with = "deserialize_object_array")]
    assertion_method: Vec<Multikey>,
    #[serde(default)]
    authentication: Vec<Multikey>,

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

impl Actor {
    fn hostname(&self) -> Result<String, ValidationError> {
        let hostname = get_hostname(&self.id)
            .map_err(|_| ValidationError("invalid actor ID"))?;
        Ok(hostname)
    }

    fn into_db_actor(self) -> DbActor {
        DbActor {
            object_type: self.object_type,
            id: self.id,
            inbox: self.inbox,
            outbox: self.outbox,
            followers: self.followers,
            subscribers: self.subscribers,
            featured: self.featured,
            url: self.url,
            gateways: self.gateways,
            public_key: None,
        }
    }
}

async fn fetch_actor_images(
    agent: &FederationAgent,
    storage: &MediaStorage,
    actor: &Actor,
    default_avatar: Option<ProfileImage>,
    default_banner: Option<ProfileImage>,
) -> Result<(Option<ProfileImage>, Option<ProfileImage>), MediaStorageError>  {
    let maybe_avatar = if let Some(icon) = &actor.icon {
        match fetch_file(
            agent,
            &icon.url,
            icon.media_type.as_deref(),
            &allowed_profile_image_media_types(&storage.supported_media_types()),
            PROFILE_IMAGE_SIZE_MAX,
        ).await {
            Ok((file_data, file_size, media_type)) => {
                let file_name = storage.save_file(file_data, &media_type)?;
                let image = ProfileImage::new(
                    file_name,
                    file_size,
                    media_type,
                );
                Some(image)
            },
            Err(error) => {
                log::warn!("failed to fetch avatar ({})", error);
                default_avatar
            },
        }
    } else {
        None
    };
    let maybe_banner = if let Some(image) = &actor.image {
        match fetch_file(
            agent,
            &image.url,
            image.media_type.as_deref(),
            &allowed_profile_image_media_types(&storage.supported_media_types()),
            PROFILE_IMAGE_SIZE_MAX,
        ).await {
            Ok((file_data, file_size, media_type)) => {
                let file_name = storage.save_file(file_data, &media_type)?;
                let image = ProfileImage::new(
                    file_name,
                    file_size,
                    media_type,
                );
                Some(image)
            },
            Err(error) => {
                log::warn!("failed to fetch banner ({})", error);
                default_banner
            },
        }
    } else {
        None
    };
    Ok((maybe_avatar, maybe_banner))
}

fn parse_public_keys(
    actor: &Actor,
) -> Result<Vec<DbActorKey>, ValidationError> {
    let mut keys = vec![];
    if actor.public_key.owner != actor.id {
        log::warn!("public key does not belong to actor");
    };
    let db_key = actor.public_key.to_db_key()?;
    keys.push(db_key);
    let verification_methods = actor.authentication.iter()
        .chain(actor.assertion_method.iter());
    for multikey in verification_methods {
        if multikey.controller == actor.id {
            let db_key = multikey.to_db_key()?;
            keys.push(db_key);
        } else {
            log::warn!("verification method does not belong to actor");
        };
    };
    keys.sort_by_key(|item| item.id.clone());
    keys.dedup_by_key(|item| item.id.clone());
    if keys.is_empty() {
        return Err(ValidationError("public keys not found"));
    };
    Ok(keys)
}

fn parse_attachments(actor: &Actor) -> (
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
                    Ok(LinkAttachment::ChatLink(field)) => {
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
        }
    };
    (identity_proofs, payment_options, proposals, extra_fields)
}

async fn fetch_proposals(
    agent: &FederationAgent,
    proposals: Vec<String>,
) -> Vec<PaymentOption> {
    let mut payment_options = vec![];
    for proposal_id in proposals {
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

fn parse_aliases(actor: &Actor) -> Vec<String> {
    // Aliases reported by server (not signed)
    actor.also_known_as.as_ref()
        .and_then(|value| {
            match parse_into_id_array(value) {
                Ok(array) => {
                    let mut aliases = vec![];
                    for actor_id in array {
                        if actor_id == actor.id ||
                            validate_object_id(&actor_id).is_err()
                        {
                            log::warn!("invalid alias: {}", actor_id);
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
    agent: &FederationAgent,
    db_client: &impl DatabaseClient,
    storage: &MediaStorage,
    actor: &Actor,
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
                agent,
                db_client,
                storage,
                tag_value,
            ).await? {
                Some(emoji) => {
                    if !emojis.contains(&emoji.id) {
                        emojis.push(emoji.id);
                    };
                },
                None => continue,
            };
        };
    };
    Ok(emojis)
}

pub async fn create_remote_profile(
    agent: &FederationAgent,
    db_client: &mut impl DatabaseClient,
    storage: &MediaStorage,
    actor_json: JsonValue,
) -> Result<DbActorProfile, HandlerError> {
    let actor: Actor = serde_json::from_value(actor_json.clone())
        .map_err(|_| ValidationError("invalid actor object"))?;
    // TODO: implement reverse webfinger lookup
    // https://swicg.github.io/activitypub-webfinger/#reverse-discovery
    let actor_hostname = actor.hostname()?;
    let (maybe_avatar, maybe_banner) = fetch_actor_images(
        agent,
        storage,
        &actor,
        None,
        None,
    ).await?;
    let public_keys = parse_public_keys(&actor)?;
    let (identity_proofs, mut payment_options, proposals, extra_fields) =
        parse_attachments(&actor);
    let subscription_options = fetch_proposals(
        agent,
        proposals,
    ).await;
    payment_options.extend(subscription_options);
    let aliases = parse_aliases(&actor);
    let emojis = parse_tags(
        agent,
        db_client,
        storage,
        &actor,
    ).await?;
    let mut profile_data = ProfileCreateData {
        username: actor.preferred_username.clone(),
        hostname: Some(actor_hostname),
        display_name: actor.name.clone(),
        bio: actor.summary.clone(),
        avatar: maybe_avatar,
        banner: maybe_banner,
        manually_approves_followers: actor.manually_approves_followers,
        mention_policy: MentionPolicy::None,
        public_keys,
        identity_proofs,
        payment_options,
        extra_fields,
        aliases,
        emojis,
        actor_json: Some(actor.into_db_actor()),
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
    agent: &FederationAgent,
    db_client: &mut impl DatabaseClient,
    storage: &MediaStorage,
    profile: DbActorProfile,
    actor_json: JsonValue,
) -> Result<DbActorProfile, HandlerError> {
    let actor: Actor = serde_json::from_value(actor_json.clone())
        .map_err(|_| ValidationError("invalid actor object"))?;
    if actor.preferred_username != profile.username {
        log::warn!("preferred username doesn't match cached value");
    };
    let actor_old = profile.actor_json
        .expect("actor data should be present");
    assert_eq!(actor_old.id, actor.id, "actor ID shouldn't change");
    let (maybe_avatar, maybe_banner) = fetch_actor_images(
        agent,
        storage,
        &actor,
        profile.avatar,
        profile.banner,
    ).await?;
    let public_keys = parse_public_keys(&actor)?;
    let (identity_proofs, mut payment_options, proposals, extra_fields) =
        parse_attachments(&actor);
    let subscription_options = fetch_proposals(
        agent,
        proposals,
    ).await;
    payment_options.extend(subscription_options);
    let aliases = parse_aliases(&actor);
    let emojis = parse_tags(
        agent,
        db_client,
        storage,
        &actor,
    ).await?;
    let mut profile_data = ProfileUpdateData {
        username: actor.preferred_username.clone(),
        display_name: actor.name.clone(),
        bio: actor.summary.clone(),
        bio_source: actor.summary.clone(),
        avatar: maybe_avatar,
        banner: maybe_banner,
        manually_approves_followers: actor.manually_approves_followers,
        mention_policy: MentionPolicy::None,
        public_keys,
        identity_proofs,
        payment_options,
        extra_fields,
        aliases,
        emojis,
        actor_json: Some(actor.into_db_actor()),
    };
    clean_profile_update_data(&mut profile_data)?;
    // update_profile() clears unreachable_since
    let (profile, deletion_queue) =
        update_profile(db_client, &profile.id, profile_data).await?;
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
    use mitra_models::profiles::types::PublicKeyType;
    use mitra_utils::{
        crypto_eddsa::{
            ed25519_public_key_from_secret_key,
            generate_ed25519_key,
        },
        crypto_rsa::{
            generate_weak_rsa_key,
            rsa_public_key_to_pkcs1_der,
        },
    };
    use crate::actors::keys::{Multikey, PublicKey};
    use super::*;

    #[test]
    fn test_get_actor_hostname() {
        let actor = Actor {
            id: "https://δοκιμή.example/users/1".to_string(),
            preferred_username: "test".to_string(),
            ..Default::default()
        };
        let hostname = actor.hostname().unwrap();
        assert_eq!(hostname, "xn--jxalpdlp.example");
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
        let actor = Actor {
            id: actor_id.to_string(),
            public_key: actor_public_key,
            authentication: vec![actor_auth_key_1, actor_auth_key_2],
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
