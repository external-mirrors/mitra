use std::collections::HashMap;

use serde::{
    Deserialize,
    Deserializer,
    Serialize,
    de::{Error as DeserializerError},
};
use serde_json::{json, Value};

use mitra_config::Instance;
use mitra_models::{
    database::{DatabaseError, DatabaseTypeError},
    profiles::types::{
        DbActor,
        DbActorPublicKey,
        ExtraField,
        IdentityProof,
        IdentityProofType,
        PaymentOption,
    },
    users::types::User,
};
use mitra_utils::{
    crypto_rsa::RsaSerializationError,
    urls::get_hostname,
};

use crate::activitypub::{
    constants::{
        AP_CONTEXT,
        MASTODON_CONTEXT,
        MITRA_CONTEXT,
        SCHEMA_ORG_CONTEXT,
        W3ID_DATA_INTEGRITY_CONTEXT,
        W3ID_MULTIKEY_CONTEXT,
        W3ID_SECURITY_CONTEXT,
    },
    deserialization::{
        parse_into_array,
        parse_into_href_array,
    },
    identifiers::{
        local_actor_id,
        local_instance_actor_id,
        LocalActorCollection,
    },
    types::deserialize_value_array,
    vocabulary::{
        IDENTITY_PROOF,
        IMAGE,
        LINK,
        NOTE,
        PERSON,
        PROPERTY_VALUE,
        SERVICE,
        VERIFIABLE_IDENTITY_STATEMENT,
    },
};
use crate::errors::ValidationError;
use crate::media::get_file_url;
use crate::webfinger::types::ActorAddress;

use super::attachments::{
    attach_extra_field,
    attach_identity_proof,
    attach_payment_option,
    parse_identity_proof,
    parse_identity_proof_fep_c390,
    parse_metadata_field,
    parse_payment_option,
    parse_property_value,
};
use super::keys::{Multikey, PublicKey};

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActorImage {
    #[serde(rename = "type")]
    object_type: String,
    pub url: String,
    pub media_type: Option<String>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActorAttachment {
    pub name: String,

    #[serde(rename = "type")]
    pub object_type: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub href: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature_algorithm: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature_value: Option<String>,
}

fn deserialize_image_opt<'de, D>(
    deserializer: D,
) -> Result<Option<ActorImage>, D::Error>
    where D: Deserializer<'de>
{
    let maybe_value: Option<Value> = Option::deserialize(deserializer)?;
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
    let maybe_value: Option<Value> = Option::deserialize(deserializer)?;
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

#[derive(Deserialize, Serialize)]
#[cfg_attr(test, derive(Default))]
#[serde(rename_all = "camelCase")]
pub struct Actor {
    #[serde(rename = "@context")]
    pub context: Option<Value>,

    pub id: String,

    #[serde(rename = "type")]
    pub object_type: String,

    pub name: Option<String>,

    pub preferred_username: String,
    pub inbox: String,
    pub outbox: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub followers: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub following: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub subscribers: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub featured: Option<String>,

    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
    )]
    pub authentication: Vec<Multikey>,

    pub public_key: PublicKey,

    #[serde(
        default,
        deserialize_with = "deserialize_image_opt",
        skip_serializing_if = "Option::is_none",
    )]
    pub icon: Option<ActorImage>,

    #[serde(
        default,
        deserialize_with = "deserialize_image_opt",
        skip_serializing_if = "Option::is_none",
    )]
    pub image: Option<ActorImage>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub also_known_as: Option<Value>,

    #[serde(
        default,
        deserialize_with = "deserialize_value_array",
        skip_serializing_if = "Vec::is_empty",
    )]
    pub attachment: Vec<Value>,

    #[serde(default)]
    pub manually_approves_followers: bool,

    #[serde(
        default,
        deserialize_with = "deserialize_value_array",
        skip_serializing_if = "Vec::is_empty",
    )]
    pub tag: Vec<Value>,

    #[serde(
        default,
        deserialize_with = "deserialize_url_opt",
        skip_serializing_if = "Option::is_none",
    )]
    pub url: Option<String>,
}

impl Actor {
    pub fn address(
        &self,
    ) -> Result<ActorAddress, ValidationError> {
        let hostname = get_hostname(&self.id)
            .map_err(|_| ValidationError("invalid actor ID"))?;
        let actor_address = ActorAddress {
            username: self.preferred_username.clone(),
            hostname: hostname,
        };
        Ok(actor_address)
    }

    pub fn into_db_actor(self) -> DbActor {
        DbActor {
            object_type: self.object_type,
            id: self.id,
            inbox: self.inbox,
            outbox: self.outbox,
            followers: self.followers,
            subscribers: self.subscribers,
            featured: self.featured,
            url: self.url,
            public_key: DbActorPublicKey {
                id: self.public_key.id,
                owner: self.public_key.owner,
                public_key_pem: self.public_key.public_key_pem,
            },
        }
    }

    pub fn parse_attachments(&self) -> (
        Vec<IdentityProof>,
        Vec<PaymentOption>,
        Vec<ExtraField>,
    ) {
        let mut identity_proofs = vec![];
        let mut payment_options = vec![];
        let mut extra_fields = vec![];
        let mut property_values = vec![];
        let log_error = |attachment_type: &str, error| {
            log::warn!(
                "ignoring actor attachment of type {}: {}",
                attachment_type,
                error,
            );
        };
        for attachment_value in self.attachment.iter() {
            let attachment_type =
                attachment_value["type"].as_str().unwrap_or("Unknown");
            match attachment_type {
                IDENTITY_PROOF => {
                    match parse_identity_proof(&self.id, attachment_value) {
                        Ok(proof) => identity_proofs.push(proof),
                        Err(error) => log_error(attachment_type, error),
                    };
                },
                VERIFIABLE_IDENTITY_STATEMENT => {
                    match parse_identity_proof_fep_c390(&self.id, attachment_value) {
                        Ok(proof) => identity_proofs.push(proof),
                        Err(error) => log_error(attachment_type, error),
                    };
                },
                LINK => {
                    match parse_payment_option(attachment_value) {
                        Ok(option) => payment_options.push(option),
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
        // FEP-8b2a fields have higher priority
        for field in property_values {
            if extra_fields.iter().any(|item| item.name == field.name) {
                continue;
            } else {
                extra_fields.push(field);
            }
        };
        (identity_proofs, payment_options, extra_fields)
    }
}

fn build_actor_context() -> (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    HashMap<&'static str, &'static str>,
) {
    (
        AP_CONTEXT,
        W3ID_SECURITY_CONTEXT,
        W3ID_DATA_INTEGRITY_CONTEXT,
        W3ID_MULTIKEY_CONTEXT,
        HashMap::from([
            ("manuallyApprovesFollowers", "as:manuallyApprovesFollowers"),
            ("schema", SCHEMA_ORG_CONTEXT),
            ("PropertyValue", "schema:PropertyValue"),
            ("value", "schema:value"),
            ("toot", MASTODON_CONTEXT),
            ("IdentityProof", "toot:IdentityProof"),
            ("featured", "toot:featured"),
            ("mitra", MITRA_CONTEXT),
            ("subscribers", "mitra:subscribers"),
            ("subject", "mitra:subject"),
            ("VerifiableIdentityStatement", "mitra:VerifiableIdentityStatement"),
        ]),
    )
}

pub fn build_local_actor(
    user: &User,
    instance_url: &str,
) -> Result<Actor, DatabaseError> {
    let username = &user.profile.username;
    let actor_id = local_actor_id(instance_url, username);
    let inbox = LocalActorCollection::Inbox.of(&actor_id);
    let outbox = LocalActorCollection::Outbox.of(&actor_id);
    let followers = LocalActorCollection::Followers.of(&actor_id);
    let following = LocalActorCollection::Following.of(&actor_id);
    let subscribers = LocalActorCollection::Subscribers.of(&actor_id);
    let featured = LocalActorCollection::Featured.of(&actor_id);

    let public_key = PublicKey::build(&actor_id, &user.rsa_private_key)
        .map_err(|_| DatabaseTypeError)?;
    let mut authentication_keys = vec![
        Multikey::build_rsa(&actor_id, &user.rsa_private_key)
            .map_err(|_| DatabaseTypeError)?,
    ];
    if let Some(ref private_key) = user.ed25519_private_key {
        let multikey = Multikey::build_ed25519(&actor_id, private_key.inner());
        authentication_keys.push(multikey);
    };
    let avatar = match &user.profile.avatar {
        Some(image) => {
            let actor_image = ActorImage {
                object_type: IMAGE.to_string(),
                url: get_file_url(instance_url, &image.file_name),
                media_type: image.media_type.clone(),
            };
            Some(actor_image)
        },
        None => None,
    };
    let banner = match &user.profile.banner {
        Some(image) => {
            let actor_image = ActorImage {
                object_type: IMAGE.to_string(),
                url: get_file_url(instance_url, &image.file_name),
                media_type: image.media_type.clone(),
            };
            Some(actor_image)
        },
        None => None,
    };
    let mut attachments = vec![];
    for proof in user.profile.identity_proofs.clone().into_inner() {
        let attachment_value = match proof.proof_type {
            IdentityProofType::LegacyEip191IdentityProof |
                IdentityProofType::LegacyMinisignIdentityProof =>
            {
                let attachment = attach_identity_proof(proof)?;
                serde_json::to_value(attachment)
                    .expect("attachment should be serializable")
            },
            _ => proof.value,
        };
        attachments.push(attachment_value);
    };
    for payment_option in user.profile.payment_options.clone().into_inner() {
        let attachment = attach_payment_option(
            instance_url,
            &user.profile.username,
            payment_option,
        );
        let attachment_value = serde_json::to_value(attachment)
            .expect("attachment should be serializable");
        attachments.push(attachment_value);
    };
    for field in user.profile.extra_fields.clone().into_inner() {
        let attachment = attach_extra_field(field);
        let attachment_value = serde_json::to_value(attachment)
            .expect("attachment should be serializable");
        attachments.push(attachment_value);
    };
    let aliases = user.profile.aliases.clone().into_actor_ids();
    let actor = Actor {
        context: Some(json!(build_actor_context())),
        id: actor_id.clone(),
        object_type: PERSON.to_string(),
        name: user.profile.display_name.clone(),
        preferred_username: username.to_string(),
        inbox,
        outbox,
        followers: Some(followers),
        following: Some(following),
        subscribers: Some(subscribers),
        featured: Some(featured),
        authentication: authentication_keys,
        public_key,
        icon: avatar,
        image: banner,
        summary: user.profile.bio.clone(),
        also_known_as: Some(json!(aliases)),
        attachment: attachments,
        manually_approves_followers: false,
        tag: vec![],
        url: Some(actor_id),
    };
    Ok(actor)
}

pub fn build_instance_actor(
    instance: &Instance,
) -> Result<Actor, RsaSerializationError> {
    let actor_id = local_instance_actor_id(&instance.url());
    let actor_inbox = LocalActorCollection::Inbox.of(&actor_id);
    let actor_outbox = LocalActorCollection::Outbox.of(&actor_id);
    let public_key = PublicKey::build(&actor_id, &instance.actor_key)?;
    let authentication_keys = vec![
        Multikey::build_rsa(&actor_id, &instance.actor_key)?,
    ];
    let actor = Actor {
        context: Some(json!(build_actor_context())),
        id: actor_id,
        object_type: SERVICE.to_string(),
        name: Some(instance.hostname()),
        preferred_username: instance.hostname(),
        inbox: actor_inbox,
        outbox: actor_outbox,
        followers: None,
        following: None,
        subscribers: None,
        featured: None,
        authentication: authentication_keys,
        public_key,
        icon: None,
        image: None,
        summary: None,
        also_known_as: None,
        attachment: vec![],
        manually_approves_followers: false,
        tag: vec![],
        url: None,
    };
    Ok(actor)
}

#[cfg(test)]
mod tests {
    use mitra_models::profiles::types::DbActorProfile;
    use super::*;

    const INSTANCE_HOSTNAME: &str = "example.com";
    const INSTANCE_URL: &str = "https://example.com";

    #[test]
    fn test_get_actor_address() {
        let actor = Actor {
            id: "https://test.org/users/1".to_string(),
            preferred_username: "test".to_string(),
            ..Default::default()
        };
        let actor_address = actor.address().unwrap();
        assert_eq!(actor_address.acct(INSTANCE_HOSTNAME), "test@test.org");
    }

    #[test]
    fn test_build_local_actor() {
        let profile = DbActorProfile {
            username: "testuser".to_string(),
            bio: Some("testbio".to_string()),
            ..Default::default()
        };
        let user = User { profile, ..Default::default() };
        let actor = build_local_actor(&user, INSTANCE_URL).unwrap();
        assert_eq!(actor.id, "https://example.com/users/testuser");
        assert_eq!(actor.preferred_username, user.profile.username);
        assert_eq!(actor.inbox, "https://example.com/users/testuser/inbox");
        assert_eq!(actor.outbox, "https://example.com/users/testuser/outbox");
        assert_eq!(
            actor.followers.unwrap(),
            "https://example.com/users/testuser/followers",
        );
        assert_eq!(
            actor.following.unwrap(),
            "https://example.com/users/testuser/following",
        );
        assert_eq!(
            actor.subscribers.unwrap(),
            "https://example.com/users/testuser/subscribers",
        );
        assert_eq!(
            actor.public_key.id,
            "https://example.com/users/testuser#main-key",
        );
        assert_eq!(actor.attachment.len(), 0);
        assert_eq!(actor.summary, user.profile.bio);
    }

    #[test]
    fn test_build_instance_actor() {
        let instance_url = "https://example.com/";
        let instance = Instance::for_test(instance_url);
        let actor = build_instance_actor(&instance).unwrap();
        assert_eq!(actor.id, "https://example.com/actor");
        assert_eq!(actor.object_type, "Service");
        assert_eq!(actor.preferred_username, "example.com");
        assert_eq!(actor.inbox, "https://example.com/actor/inbox");
        assert_eq!(actor.outbox, "https://example.com/actor/outbox");
        assert_eq!(actor.public_key.id, "https://example.com/actor#main-key");
    }
}
