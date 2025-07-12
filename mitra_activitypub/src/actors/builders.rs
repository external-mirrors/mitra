use apx_core::{
    crypto_rsa::RsaSerializationError,
};
use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::{Value as JsonValue};

use mitra_config::Instance;
use mitra_models::{
    database::{DatabaseError, DatabaseTypeError},
    profiles::types::IdentityProofType,
    users::types::User,
};
use mitra_services::media::MediaServer;

use crate::{
    authority::Authority,
    builders::emoji::{build_emoji, Emoji},
    contexts::{
        AP_CONTEXT,
        MASTODON_CONTEXT,
        MITRA_CONTEXT,
        SCHEMA_ORG_CONTEXT,
        W3C_CID_CONTEXT,
        W3ID_DATA_INTEGRITY_CONTEXT,
        W3ID_SECURITY_CONTEXT,
    },
    identifiers::{
        local_actor_id,
        local_actor_id_unified,
        local_instance_actor_id,
        LocalActorCollection,
    },
    keys::{Multikey, PublicKeyPem},
    vocabulary::{APPLICATION, IMAGE, PERSON, SERVICE},
};

use super::attachments::{
    attach_extra_field,
    attach_payment_option,
};

type Context = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    IndexMap<&'static str, &'static str>,
);

fn build_actor_context() -> Context {
    (
        AP_CONTEXT,
        W3C_CID_CONTEXT,
        W3ID_SECURITY_CONTEXT,
        W3ID_DATA_INTEGRITY_CONTEXT,
        IndexMap::from([
            ("manuallyApprovesFollowers", "as:manuallyApprovesFollowers"),
            ("schema", SCHEMA_ORG_CONTEXT),
            ("PropertyValue", "schema:PropertyValue"),
            ("value", "schema:value"),
            ("toot", MASTODON_CONTEXT),
            ("featured", "toot:featured"),
            ("Emoji", "toot:Emoji"),
            ("mitra", MITRA_CONTEXT),
            ("subscribers", "mitra:subscribers"),
            ("VerifiableIdentityStatement", "mitra:VerifiableIdentityStatement"),
            ("MitraJcsEip191Signature2022", "mitra:MitraJcsEip191Signature2022"),
            ("gateways", "mitra:gateways"),
            ("implements", "mitra:implements"),
            // Workarounds for MitraJcsEip191Signature2022
            // (not required for DataIntegrityProof)
            ("proofValue", "sec:proofValue"),
            ("proofPurpose", "sec:proofPurpose"),
            // With DID context:
            // "Invalid JSON-LD syntax; tried to redefine a protected term."
            //("verificationMethod", "sec:verificationMethod"),
        ]),
    )
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActorImage {
    #[serde(rename = "type")]
    pub object_type: String,
    pub url: String,
    pub media_type: Option<String>,
}

#[derive(Serialize)]
struct ApplicationFeature {
    name: &'static str,
    href: &'static str,
}

#[derive(Serialize)]
struct Application {
    #[serde(rename = "type")]
    object_type: &'static str,
    implements: Vec<ApplicationFeature>,
}

impl Application {
    fn new() -> Self {
        let rfc9421 = ApplicationFeature {
            name: "RFC-9421: HTTP Message Signatures",
            href: "https://datatracker.ietf.org/doc/html/rfc9421",
        };
        let rfc9421_ed25519 = ApplicationFeature {
            name: "RFC-9421 signatures using the Ed25519 algorithm",
            href: "https://datatracker.ietf.org/doc/html/rfc9421#name-eddsa-using-curve-edwards25",
        };
        Self {
            object_type: APPLICATION,
            implements: vec![rfc9421, rfc9421_ed25519],
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Actor {
    #[serde(rename = "@context")]
    _context: Context,

    pub id: String,

    #[serde(rename = "type")]
    object_type: String,

    name: Option<String>,
    preferred_username: String,

    inbox: String,
    outbox: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    followers: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    following: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    subscribers: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    featured: Option<String>,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    assertion_method: Vec<Multikey>,

    public_key: PublicKeyPem,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    implements: Vec<ApplicationFeature>,
    #[serde(skip_serializing_if = "Option::is_none")]
    generator: Option<Application>,

    #[serde(skip_serializing_if = "Option::is_none")]
    icon: Option<ActorImage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    image: Option<ActorImage>,

    #[serde(skip_serializing_if = "Option::is_none")]
    summary: Option<String>,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    also_known_as: Vec<String>,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    attachment: Vec<JsonValue>,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    tag: Vec<Emoji>,

    manually_approves_followers: bool,

    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    published: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    updated: Option<DateTime<Utc>>,

    // Required for FEP-ef61
    #[serde(skip_serializing_if = "Vec::is_empty")]
    gateways: Vec<String>,
}

pub fn build_local_actor(
    instance_url: &str,
    authority: &Authority,
    media_server: &MediaServer,
    user: &User,
) -> Result<Actor, DatabaseError> {
    assert_eq!(authority.server_url(), instance_url);
    let username = &user.profile.username;
    let actor_id = local_actor_id_unified(authority, username);
    let actor_type = if user.profile.is_automated {
        SERVICE
    } else {
        PERSON
    };
    let inbox = LocalActorCollection::Inbox.of(&actor_id);
    let outbox = LocalActorCollection::Outbox.of(&actor_id);
    let followers = LocalActorCollection::Followers.of(&actor_id);
    let following = LocalActorCollection::Following.of(&actor_id);
    let subscribers = LocalActorCollection::Subscribers.of(&actor_id);
    let featured = LocalActorCollection::Featured.of(&actor_id);

    let public_key = PublicKeyPem::build(&actor_id, &user.rsa_secret_key)
        .map_err(|_| DatabaseTypeError)?;
    let verification_methods = vec![
        Multikey::build_rsa(&actor_id, &user.rsa_secret_key)
            .map_err(|_| DatabaseTypeError)?,
        Multikey::build_ed25519(&actor_id, &user.ed25519_secret_key),
    ];
    let avatar = match &user.profile.avatar {
        Some(image) => {
            let actor_image = ActorImage {
                object_type: IMAGE.to_string(),
                url: media_server.url_for(&image.file_name),
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
                url: media_server.url_for(&image.file_name),
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
                // Don't attach legacy identity proofs
                continue;
            },
            _ => proof.value,
        };
        attachments.push(attachment_value);
    };
    for payment_option in user.profile.payment_options.clone().into_inner() {
        let attachment = attach_payment_option(
            authority,
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
    let mut emojis = vec![];
    for db_emoji in user.profile.emojis.clone().into_inner() {
        let emoji = build_emoji(instance_url, media_server, &db_emoji);
        emojis.push(emoji);
    };
    let aliases = user.profile.aliases.clone().into_actor_ids();
    // HTML representation
    // TODO: portable actors should point to a primary server
    let profile_url = local_actor_id(instance_url, username);

    let gateways = authority.is_fep_ef61()
        .then_some(vec![instance_url.to_string()])
        .unwrap_or_default();
    let actor = Actor {
        _context: build_actor_context(),
        id: actor_id.clone(),
        object_type: actor_type.to_string(),
        name: user.profile.display_name.clone(),
        preferred_username: username.clone(),
        inbox,
        outbox,
        followers: Some(followers),
        following: Some(following),
        subscribers: Some(subscribers),
        featured: Some(featured),
        assertion_method: verification_methods,
        public_key,
        implements: vec![],
        generator: Some(Application::new()),
        icon: avatar,
        image: banner,
        summary: user.profile.bio.clone(),
        also_known_as: aliases,
        attachment: attachments,
        tag: emojis,
        manually_approves_followers: user.profile.manually_approves_followers,
        url: Some(profile_url),
        published: Some(user.profile.created_at),
        updated: Some(user.profile.updated_at),
        gateways: gateways,
    };
    Ok(actor)
}

pub fn build_instance_actor(
    instance: &Instance,
) -> Result<Actor, RsaSerializationError> {
    let actor_id = local_instance_actor_id(&instance.url());
    let actor_inbox = LocalActorCollection::Inbox.of(&actor_id);
    let actor_outbox = LocalActorCollection::Outbox.of(&actor_id);
    let public_key = PublicKeyPem::build(&actor_id, &instance.actor_rsa_key)?;
    let verification_methods = vec![
        Multikey::build_rsa(&actor_id, &instance.actor_rsa_key)?,
        Multikey::build_ed25519(&actor_id, &instance.actor_ed25519_key),
    ];
    let actor = Actor {
        _context: build_actor_context(),
        id: actor_id,
        object_type: APPLICATION.to_string(),
        name: Some(instance.hostname()),
        preferred_username: instance.hostname(),
        inbox: actor_inbox,
        outbox: actor_outbox,
        followers: None,
        following: None,
        subscribers: None,
        featured: None,
        assertion_method: verification_methods,
        public_key,
        implements: Application::new().implements,
        generator: None,
        icon: None,
        image: None,
        summary: None,
        also_known_as: vec![],
        attachment: vec![],
        tag: vec![],
        manually_approves_followers: false,
        url: None,
        published: None,
        updated: None,
        gateways: vec![],
    };
    Ok(actor)
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use mitra_models::profiles::types::DbActorProfile;
    use super::*;

    const INSTANCE_URL: &str = "https://server.example";

    #[test]
    fn test_build_local_actor() {
        let mut profile = DbActorProfile::local_for_test("testuser");
        profile.bio = Some("testbio".to_string());
        profile.created_at = DateTime::parse_from_rfc3339("2023-02-24T23:36:38Z")
            .unwrap()
            .with_timezone(&Utc);
        profile.updated_at = profile.created_at;
        let user = User { profile, ..Default::default() };
        let authority = Authority::from_user(INSTANCE_URL, &user, false);
        let media_server = MediaServer::for_test(INSTANCE_URL);
        let actor = build_local_actor(
            INSTANCE_URL,
            &authority,
            &media_server,
            &user,
        ).unwrap();
        let value = serde_json::to_value(actor).unwrap();
        let expected_value = json!({
            "@context": [
                "https://www.w3.org/ns/activitystreams",
                "https://www.w3.org/ns/cid/v1",
                "https://w3id.org/security/v1",
                "https://w3id.org/security/data-integrity/v1",
                {
                    "manuallyApprovesFollowers": "as:manuallyApprovesFollowers",
                    "schema": "http://schema.org/",
                    "PropertyValue": "schema:PropertyValue",
                    "value": "schema:value",
                    "toot": "http://joinmastodon.org/ns#",
                    "featured": "toot:featured",
                    "Emoji": "toot:Emoji",
                    "mitra": "http://jsonld.mitra.social#",
                    "subscribers": "mitra:subscribers",
                    "VerifiableIdentityStatement": "mitra:VerifiableIdentityStatement",
                    "MitraJcsEip191Signature2022": "mitra:MitraJcsEip191Signature2022",
                    "gateways": "mitra:gateways",
                    "implements": "mitra:implements",
                    "proofValue": "sec:proofValue",
                    "proofPurpose": "sec:proofPurpose",
                },
            ],
            "id": "https://server.example/users/testuser",
            "type": "Person",
            "name": null,
            "preferredUsername": "testuser",
            "inbox": "https://server.example/users/testuser/inbox",
            "outbox": "https://server.example/users/testuser/outbox",
            "followers": "https://server.example/users/testuser/followers",
            "following": "https://server.example/users/testuser/following",
            "subscribers": "https://server.example/users/testuser/subscribers",
            "featured": "https://server.example/users/testuser/collections/featured",
            "assertionMethod": [
                {
                    "id": "https://server.example/users/testuser#main-key",
                    "type": "Multikey",
                    "controller": "https://server.example/users/testuser",
                    "publicKeyMultibase": "zDrrewXm1cTFaEwruJq4sA7sPhxciancezhnoCxrdvSLs3gQSupJxKA719sQGmG71CkuQdnDxAUpecZ1b7fYQTTrhKA7KbdxWUPRXqs3e",
                },
                {
                    "id": "https://server.example/users/testuser#ed25519-key",
                    "type": "Multikey",
                    "controller": "https://server.example/users/testuser",
                    "publicKeyMultibase": "z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6",
                },
            ],
            "publicKey": {
                "id": "https://server.example/users/testuser#main-key",
                "owner": "https://server.example/users/testuser",
                "publicKeyPem": "-----BEGIN PUBLIC KEY-----\nMFwwDQYJKoZIhvcNAQEBBQADSwAwSAJBAOIh58ZQbo45MuZvv1nMWAzTzN9oghNC\nbxJkFEFD1Y49LEeNHMk6GrPByUz8kn4y8Hf6brb+DVm7ZW4cdhOx1TsCAwEAAQ==\n-----END PUBLIC KEY-----\n",
            },
            "generator": {
                "type": "Application",
                "implements": [
                    {
                        "name": "RFC-9421: HTTP Message Signatures",
                        "href": "https://datatracker.ietf.org/doc/html/rfc9421",
                    },
                    {
                        "name": "RFC-9421 signatures using the Ed25519 algorithm",
                        "href": "https://datatracker.ietf.org/doc/html/rfc9421#name-eddsa-using-curve-edwards25",
                    },
                ],
            },
            "summary": "testbio",
            "manuallyApprovesFollowers": false,
            "url": "https://server.example/users/testuser",
            "published": "2023-02-24T23:36:38Z",
            "updated": "2023-02-24T23:36:38Z",
        });
        assert_eq!(value, expected_value);
    }

    #[test]
    fn test_build_local_actor_fep_ef61() {
        let mut profile = DbActorProfile::local_for_test("testuser");
        profile.bio = Some("testbio".to_string());
        profile.created_at = DateTime::parse_from_rfc3339("2023-02-24T23:36:38Z")
            .unwrap()
            .with_timezone(&Utc);
        profile.updated_at = profile.created_at;
        let user = User { profile, ..Default::default() };
        let authority = Authority::from_user(INSTANCE_URL, &user, true);
        let media_server = MediaServer::for_test(INSTANCE_URL);
        let actor = build_local_actor(
            INSTANCE_URL,
            &authority,
            &media_server,
            &user,
        ).unwrap();
        let value = serde_json::to_value(actor).unwrap();
        let expected_value = json!({
            "@context": [
                "https://www.w3.org/ns/activitystreams",
                "https://www.w3.org/ns/cid/v1",
                "https://w3id.org/security/v1",
                "https://w3id.org/security/data-integrity/v1",
                {
                    "manuallyApprovesFollowers": "as:manuallyApprovesFollowers",
                    "schema": "http://schema.org/",
                    "PropertyValue": "schema:PropertyValue",
                    "value": "schema:value",
                    "toot": "http://joinmastodon.org/ns#",
                    "featured": "toot:featured",
                    "Emoji": "toot:Emoji",
                    "mitra": "http://jsonld.mitra.social#",
                    "subscribers": "mitra:subscribers",
                    "VerifiableIdentityStatement": "mitra:VerifiableIdentityStatement",
                    "MitraJcsEip191Signature2022": "mitra:MitraJcsEip191Signature2022",
                    "gateways": "mitra:gateways",
                    "implements": "mitra:implements",
                    "proofValue": "sec:proofValue",
                    "proofPurpose": "sec:proofPurpose",
                },
            ],
            "id": "https://server.example/.well-known/apgateway/did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor",
            "type": "Person",
            "name": null,
            "preferredUsername": "testuser",
            "inbox": "https://server.example/.well-known/apgateway/did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor/inbox",
            "outbox": "https://server.example/.well-known/apgateway/did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor/outbox",
            "followers": "https://server.example/.well-known/apgateway/did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor/followers",
            "following": "https://server.example/.well-known/apgateway/did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor/following",
            "subscribers": "https://server.example/.well-known/apgateway/did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor/subscribers",
            "featured": "https://server.example/.well-known/apgateway/did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor/collections/featured",
            "assertionMethod": [
                {
                    "id": "https://server.example/.well-known/apgateway/did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor#main-key",
                    "type": "Multikey",
                    "controller": "https://server.example/.well-known/apgateway/did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor",
                    "publicKeyMultibase": "zDrrewXm1cTFaEwruJq4sA7sPhxciancezhnoCxrdvSLs3gQSupJxKA719sQGmG71CkuQdnDxAUpecZ1b7fYQTTrhKA7KbdxWUPRXqs3e",
                },
                {
                    "id": "https://server.example/.well-known/apgateway/did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor#ed25519-key",
                    "type": "Multikey",
                    "controller": "https://server.example/.well-known/apgateway/did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor",
                    "publicKeyMultibase": "z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6",
                },
            ],
            "publicKey": {
                "id": "https://server.example/.well-known/apgateway/did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor#main-key",
                "owner": "https://server.example/.well-known/apgateway/did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor",
                "publicKeyPem": "-----BEGIN PUBLIC KEY-----\nMFwwDQYJKoZIhvcNAQEBBQADSwAwSAJBAOIh58ZQbo45MuZvv1nMWAzTzN9oghNC\nbxJkFEFD1Y49LEeNHMk6GrPByUz8kn4y8Hf6brb+DVm7ZW4cdhOx1TsCAwEAAQ==\n-----END PUBLIC KEY-----\n",
            },
            "generator": {
                "type": "Application",
                "implements": [
                    {
                        "name": "RFC-9421: HTTP Message Signatures",
                        "href": "https://datatracker.ietf.org/doc/html/rfc9421",
                    },
                    {
                        "name": "RFC-9421 signatures using the Ed25519 algorithm",
                        "href": "https://datatracker.ietf.org/doc/html/rfc9421#name-eddsa-using-curve-edwards25",
                    },
                ],
            },
            "summary": "testbio",
            "manuallyApprovesFollowers": false,
            "url": "https://server.example/users/testuser",
            "published": "2023-02-24T23:36:38Z",
            "updated": "2023-02-24T23:36:38Z",
            "gateways": [
                "https://server.example"
            ],
        });
        assert_eq!(value, expected_value);
    }

    #[test]
    fn test_build_instance_actor() {
        let instance_url = "https://server.example/";
        let instance = Instance::for_test(instance_url);
        let actor = build_instance_actor(&instance).unwrap();
        let value = serde_json::to_value(actor).unwrap();
        let expected_value = json!({
            "@context": [
                "https://www.w3.org/ns/activitystreams",
                "https://www.w3.org/ns/cid/v1",
                "https://w3id.org/security/v1",
                "https://w3id.org/security/data-integrity/v1",
                {
                    "manuallyApprovesFollowers": "as:manuallyApprovesFollowers",
                    "schema": "http://schema.org/",
                    "PropertyValue": "schema:PropertyValue",
                    "value": "schema:value",
                    "toot": "http://joinmastodon.org/ns#",
                    "featured": "toot:featured",
                    "Emoji": "toot:Emoji",
                    "mitra": "http://jsonld.mitra.social#",
                    "subscribers": "mitra:subscribers",
                    "VerifiableIdentityStatement": "mitra:VerifiableIdentityStatement",
                    "MitraJcsEip191Signature2022": "mitra:MitraJcsEip191Signature2022",
                    "gateways": "mitra:gateways",
                    "implements": "mitra:implements",
                    "proofValue": "sec:proofValue",
                    "proofPurpose": "sec:proofPurpose",
                },
            ],
            "id": "https://server.example/actor",
            "type": "Application",
            "name": "server.example",
            "preferredUsername": "server.example",
            "inbox": "https://server.example/actor/inbox",
            "outbox": "https://server.example/actor/outbox",
            "assertionMethod": [
                {
                    "id": "https://server.example/actor#main-key",
                    "type": "Multikey",
                    "controller": "https://server.example/actor",
                    "publicKeyMultibase": "zDrrewXm1cTFaEwruJq4sA7sPhxciancezhnoCxrdvSLs3gQSupJxKA719sQGmG71CkuQdnDxAUpecZ1b7fYQTTrhKA7KbdxWUPRXqs3e",
                },
                {
                    "id": "https://server.example/actor#ed25519-key",
                    "type": "Multikey",
                    "controller": "https://server.example/actor",
                    "publicKeyMultibase": "z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6",
                },
            ],
            "publicKey": {
                "id": "https://server.example/actor#main-key",
                "owner": "https://server.example/actor",
                "publicKeyPem": "-----BEGIN PUBLIC KEY-----\nMFwwDQYJKoZIhvcNAQEBBQADSwAwSAJBAOIh58ZQbo45MuZvv1nMWAzTzN9oghNC\nbxJkFEFD1Y49LEeNHMk6GrPByUz8kn4y8Hf6brb+DVm7ZW4cdhOx1TsCAwEAAQ==\n-----END PUBLIC KEY-----\n",
            },
            "implements": [
                {
                    "name": "RFC-9421: HTTP Message Signatures",
                    "href": "https://datatracker.ietf.org/doc/html/rfc9421",
                },
                {
                    "name": "RFC-9421 signatures using the Ed25519 algorithm",
                    "href": "https://datatracker.ietf.org/doc/html/rfc9421#name-eddsa-using-curve-edwards25",
                },
            ],
            "manuallyApprovesFollowers": false,
        });
        assert_eq!(value, expected_value);
    }
}
