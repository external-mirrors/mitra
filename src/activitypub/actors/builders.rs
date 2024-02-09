use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{Value as JsonValue};

use mitra_config::Instance;
use mitra_models::{
    database::{DatabaseError, DatabaseTypeError},
    profiles::types::IdentityProofType,
    users::types::User,
};
use mitra_services::media::get_file_url;
use mitra_utils::{
    crypto_eddsa::{
        ed25519_public_key_from_private_key,
        Ed25519PrivateKey,
    },
    crypto_rsa::RsaSerializationError,
    did_key::DidKey,
    json_signatures::create::sign_object_eddsa,
};

use crate::activitypub::{
    contexts::{
        AP_CONTEXT,
        MASTODON_CONTEXT,
        MITRA_CONTEXT,
        SCHEMA_ORG_CONTEXT,
        W3C_DID_CONTEXT,
        W3ID_DATA_INTEGRITY_CONTEXT,
        W3ID_MULTIKEY_CONTEXT,
        W3ID_SECURITY_CONTEXT,
    },
    identifiers::{
        local_actor_id,
        local_instance_actor_id,
        LocalActorCollection,
    },
    vocabulary::{APPLICATION, IMAGE, PERSON},
};

use super::attachments::{
    attach_extra_field,
    attach_identity_proof,
    attach_payment_option,
};
use super::keys::{Multikey, PublicKey};
use super::types::ActorImage;

type Context = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    HashMap<&'static str, &'static str>,
);

fn build_actor_context() -> Context {
    (
        AP_CONTEXT,
        W3C_DID_CONTEXT,
        W3ID_SECURITY_CONTEXT,
        W3ID_DATA_INTEGRITY_CONTEXT,
        W3ID_MULTIKEY_CONTEXT,
        HashMap::from([
            ("manuallyApprovesFollowers", "as:manuallyApprovesFollowers"),
            ("schema", SCHEMA_ORG_CONTEXT),
            ("PropertyValue", "schema:PropertyValue"),
            ("value", "schema:value"),
            ("sameAs", "schema:sameAs"),
            ("toot", MASTODON_CONTEXT),
            ("IdentityProof", "toot:IdentityProof"),
            ("featured", "toot:featured"),
            ("mitra", MITRA_CONTEXT),
            ("subscribers", "mitra:subscribers"),
            ("VerifiableIdentityStatement", "mitra:VerifiableIdentityStatement"),
            ("MitraJcsEip191Signature2022", "mitra:MitraJcsEip191Signature2022"),
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
    #[serde(skip_serializing_if = "Vec::is_empty")]
    authentication: Vec<Multikey>,

    public_key: PublicKey,

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

    manually_approves_followers: bool,

    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<String>,

    // Required for FEP-ef61
    #[serde(skip_serializing_if = "Vec::is_empty")]
    same_as: Vec<String>,
}

fn fep_ef61_identity(secret_key: Ed25519PrivateKey) -> String {
    let public_key = ed25519_public_key_from_private_key(&secret_key);
    let did_key = DidKey::from_ed25519_key(public_key.as_bytes());
    format!(
        "did:ap:key:{}",
        did_key.key_multibase(),
    )
}

/// Constructs canonical actor ID
fn fep_ef61_actor_id(secret_key: Ed25519PrivateKey) -> String {
    format!("{}/actor", fep_ef61_identity(secret_key))
}

// TODO: replace with regular local ID
fn fep_ef61_actor_fallback_id(instance_url: &str, username: &str) -> String {
    let actor_id =  local_actor_id(instance_url, username);
    format!("{}/fep_ef61", actor_id)
}

pub fn build_local_actor(
    instance_url: &str,
    user: &User,
    fep_ef61_enabled: bool,
) -> Result<Actor, DatabaseError> {
    let username = &user.profile.username;
    let actor_id = if fep_ef61_enabled {
        fep_ef61_actor_id(user.ed25519_private_key)
    } else {
        local_actor_id(instance_url, username)
    };
    let inbox = LocalActorCollection::Inbox.of(&actor_id);
    let outbox = LocalActorCollection::Outbox.of(&actor_id);
    let followers = LocalActorCollection::Followers.of(&actor_id);
    let following = LocalActorCollection::Following.of(&actor_id);
    let subscribers = LocalActorCollection::Subscribers.of(&actor_id);
    let featured = LocalActorCollection::Featured.of(&actor_id);

    let public_key = PublicKey::build(&actor_id, &user.rsa_private_key)
        .map_err(|_| DatabaseTypeError)?;
    let verification_methods = vec![
        Multikey::build_rsa(&actor_id, &user.rsa_private_key)
            .map_err(|_| DatabaseTypeError)?,
        Multikey::build_ed25519(&actor_id, &user.ed25519_private_key),
    ];
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
    // HTML representation
    let profile_url = local_actor_id(instance_url, username);

    let same_as = if fep_ef61_enabled {
        let url = fep_ef61_actor_fallback_id(instance_url, username);
        vec![url]
    } else {
        vec![]
    };
    let actor = Actor {
        _context: build_actor_context(),
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
        assertion_method: verification_methods.clone(),
        authentication: verification_methods,
        public_key,
        icon: avatar,
        image: banner,
        summary: user.profile.bio.clone(),
        also_known_as: aliases,
        attachment: attachments,
        manually_approves_followers: user.profile.manually_approves_followers,
        url: Some(profile_url),
        same_as: same_as,
    };
    Ok(actor)
}

pub fn build_local_actor_fep_ef61(
    instance_url: &str,
    user: &User,
    current_time: Option<DateTime<Utc>>,
) -> Result<JsonValue, DatabaseError> {
    let actor = build_local_actor(instance_url, user, true)?;
    let actor_value = serde_json::to_value(actor)
        .expect("actor should be serializable");
    let ed25519_secret_key = user.ed25519_private_key;
    // Key ID is DID
    let ed25519_key_id = fep_ef61_identity(ed25519_secret_key);
    let signed_actor = sign_object_eddsa(
        &ed25519_secret_key,
        &ed25519_key_id,
        &actor_value,
        current_time,
        false, // use eddsa-jcs-2022
    ).expect("actor object should be ready for signing");
    Ok(signed_actor)
}

pub fn build_instance_actor(
    instance: &Instance,
) -> Result<Actor, RsaSerializationError> {
    let actor_id = local_instance_actor_id(&instance.url());
    let actor_inbox = LocalActorCollection::Inbox.of(&actor_id);
    let actor_outbox = LocalActorCollection::Outbox.of(&actor_id);
    let public_key = PublicKey::build(&actor_id, &instance.actor_rsa_key)?;
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
        authentication: verification_methods.clone(),
        assertion_method: verification_methods,
        public_key,
        icon: None,
        image: None,
        summary: None,
        also_known_as: vec![],
        attachment: vec![],
        manually_approves_followers: false,
        url: None,
        same_as: vec![],
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
        let profile = DbActorProfile {
            username: "testuser".to_string(),
            bio: Some("testbio".to_string()),
            ..Default::default()
        };
        let user = User { profile, ..Default::default() };
        let actor = build_local_actor(INSTANCE_URL, &user, false).unwrap();
        let value = serde_json::to_value(actor).unwrap();
        let expected_value = json!({
            "@context": [
                "https://www.w3.org/ns/activitystreams",
                "https://www.w3.org/ns/did/v1",
                "https://w3id.org/security/v1",
                "https://w3id.org/security/data-integrity/v1",
                "https://w3id.org/security/multikey/v1",
                {
                    "manuallyApprovesFollowers": "as:manuallyApprovesFollowers",
                    "schema": "http://schema.org/",
                    "PropertyValue": "schema:PropertyValue",
                    "value": "schema:value",
                    "sameAs": "schema:sameAs",
                    "toot": "http://joinmastodon.org/ns#",
                    "IdentityProof": "toot:IdentityProof",
                    "featured": "toot:featured",
                    "mitra": "http://jsonld.mitra.social#",
                    "subscribers": "mitra:subscribers",
                    "VerifiableIdentityStatement": "mitra:VerifiableIdentityStatement",
                    "MitraJcsEip191Signature2022": "mitra:MitraJcsEip191Signature2022",
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
            "authentication": [
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
            "summary": "testbio",
            "manuallyApprovesFollowers": false,
            "url": "https://server.example/users/testuser",
        });
        assert_eq!(value, expected_value);
    }

    #[test]
    fn test_build_local_actor_fep_ef61() {
        let profile = DbActorProfile {
            username: "testuser".to_string(),
            bio: Some("testbio".to_string()),
            ..Default::default()
        };
        let user = User { profile, ..Default::default() };
        let current_time = DateTime::parse_from_rfc3339("2023-02-24T23:36:38Z")
            .unwrap().with_timezone(&Utc);
        let actor = build_local_actor_fep_ef61(
            INSTANCE_URL,
            &user,
            Some(current_time),
        ).unwrap();
        let expected_value = json!({
            "@context": [
                "https://www.w3.org/ns/activitystreams",
                "https://www.w3.org/ns/did/v1",
                "https://w3id.org/security/v1",
                "https://w3id.org/security/data-integrity/v1",
                "https://w3id.org/security/multikey/v1",
                {
                    "manuallyApprovesFollowers": "as:manuallyApprovesFollowers",
                    "schema": "http://schema.org/",
                    "PropertyValue": "schema:PropertyValue",
                    "value": "schema:value",
                    "sameAs": "schema:sameAs",
                    "toot": "http://joinmastodon.org/ns#",
                    "IdentityProof": "toot:IdentityProof",
                    "featured": "toot:featured",
                    "mitra": "http://jsonld.mitra.social#",
                    "subscribers": "mitra:subscribers",
                    "VerifiableIdentityStatement": "mitra:VerifiableIdentityStatement",
                    "MitraJcsEip191Signature2022": "mitra:MitraJcsEip191Signature2022",
                    "proofValue": "sec:proofValue",
                    "proofPurpose": "sec:proofPurpose",
                },
            ],
            "id": "did:ap:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor",
            "type": "Person",
            "name": null,
            "preferredUsername": "testuser",
            "inbox": "did:ap:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor/inbox",
            "outbox": "did:ap:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor/outbox",
            "followers": "did:ap:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor/followers",
            "following": "did:ap:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor/following",
            "subscribers": "did:ap:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor/subscribers",
            "featured": "did:ap:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor/collections/featured",
            "authentication": [
                {
                    "id": "did:ap:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor#main-key",
                    "type": "Multikey",
                    "controller": "did:ap:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor",
                    "publicKeyMultibase": "zDrrewXm1cTFaEwruJq4sA7sPhxciancezhnoCxrdvSLs3gQSupJxKA719sQGmG71CkuQdnDxAUpecZ1b7fYQTTrhKA7KbdxWUPRXqs3e",
                },
                {
                    "id": "did:ap:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor#ed25519-key",
                    "type": "Multikey",
                    "controller": "did:ap:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor",
                    "publicKeyMultibase": "z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6",
                },
            ],
            "assertionMethod": [
                {
                    "id": "did:ap:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor#main-key",
                    "type": "Multikey",
                    "controller": "did:ap:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor",
                    "publicKeyMultibase": "zDrrewXm1cTFaEwruJq4sA7sPhxciancezhnoCxrdvSLs3gQSupJxKA719sQGmG71CkuQdnDxAUpecZ1b7fYQTTrhKA7KbdxWUPRXqs3e",
                },
                {
                    "id": "did:ap:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor#ed25519-key",
                    "type": "Multikey",
                    "controller": "did:ap:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor",
                    "publicKeyMultibase": "z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6",
                },
            ],
            "publicKey": {
                "id": "did:ap:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor#main-key",
                "owner": "did:ap:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor",
                "publicKeyPem": "-----BEGIN PUBLIC KEY-----\nMFwwDQYJKoZIhvcNAQEBBQADSwAwSAJBAOIh58ZQbo45MuZvv1nMWAzTzN9oghNC\nbxJkFEFD1Y49LEeNHMk6GrPByUz8kn4y8Hf6brb+DVm7ZW4cdhOx1TsCAwEAAQ==\n-----END PUBLIC KEY-----\n",
            },
            "summary": "testbio",
            "manuallyApprovesFollowers": false,
            "url": "https://server.example/users/testuser",
            "sameAs": [
                "https://server.example/users/testuser/fep_ef61",
            ],
            "proof": {
                "created": "2023-02-24T23:36:38Z",
                "cryptosuite": "eddsa-jcs-2022",
                "proofPurpose": "assertionMethod",
                "proofValue": "z2enf2NRLmErQtHvxWxzfLZULb4hSpnXi7yKf3BCbt2G5vZNbTB9AkicaDcHKjnbcD1JX96GSRR728FUoXdCF8Ypk",
                "type": "DataIntegrityProof",
                "verificationMethod": "did:ap:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6",
            },
        });
        assert_eq!(actor, expected_value);
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
                "https://www.w3.org/ns/did/v1",
                "https://w3id.org/security/v1",
                "https://w3id.org/security/data-integrity/v1",
                "https://w3id.org/security/multikey/v1",
                {
                    "manuallyApprovesFollowers": "as:manuallyApprovesFollowers",
                    "schema": "http://schema.org/",
                    "PropertyValue": "schema:PropertyValue",
                    "value": "schema:value",
                    "sameAs": "schema:sameAs",
                    "toot": "http://joinmastodon.org/ns#",
                    "IdentityProof": "toot:IdentityProof",
                    "featured": "toot:featured",
                    "mitra": "http://jsonld.mitra.social#",
                    "subscribers": "mitra:subscribers",
                    "VerifiableIdentityStatement": "mitra:VerifiableIdentityStatement",
                    "MitraJcsEip191Signature2022": "mitra:MitraJcsEip191Signature2022",
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
            "authentication": [
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
            "manuallyApprovesFollowers": false,
        });
        assert_eq!(value, expected_value);
    }
}
