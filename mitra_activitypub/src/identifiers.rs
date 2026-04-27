use apx_core::{
    caip2::ChainId,
    url::{
        canonical::CanonicalUri,
        common::url_encode,
    },
};
use apx_sdk::identifiers::parse_object_id;
use regex::Regex;
use uuid::Uuid;

use mitra_models::{
    database::DatabaseTypeError,
    posts::types::PostDetailed,
    profiles::types::{
        DbActor,
        DbActorProfile,
        PublicKeyType,
    },
};
use mitra_validators::errors::ValidationError;

use crate::{
    authority::{Authority, AuthorityRoot},
    utils::parse_id_from_db_lenient,
};

pub enum LocalActorCollection {
    Inbox,
    Outbox,
    Followers,
    Following,
    Subscribers,
    Featured,
}

impl LocalActorCollection {
    pub fn of(&self, actor_id: &str) -> String {
        let name = match self {
            Self::Inbox => "inbox",
            Self::Outbox => "outbox",
            Self::Followers => "followers",
            Self::Following => "following",
            // TODO: collections/subscribers
            Self::Subscribers => "subscribers",
            Self::Featured => "collections/featured",
        };
        format!("{}/{}", actor_id, name)
    }
}

// Mastodon and Pleroma use the same actor ID format
pub fn local_actor_id(instance_uri: &str, username: &str) -> String {
    format!("{}/users/{}", instance_uri, username)
}

pub fn local_actor_id_unified(
    authority: &Authority,
    internal_id: Uuid,
    username: &str,
) -> String {
    match authority.root() {
        AuthorityRoot::Server(_) => local_actor_id(&authority.to_string(), username),
        AuthorityRoot::Key(_) => format!("{}/actors/{}", authority, internal_id),
    }
}

pub fn local_instance_actor_id(instance_uri: &str) -> String {
    format!("{}/actor", instance_uri)
}

pub fn local_actor_key_id(
    actor_id: &str,
    key_type: PublicKeyType,
) -> String {
    let fragment = match key_type {
        PublicKeyType::RsaPkcs1 => "#main-key",
        PublicKeyType::Ed25519 => "#ed25519-key",
    };
    format!("{}{}", actor_id, fragment)
}

pub fn local_actor_proposal_id(
    actor_id: &str,
    chain_id: &ChainId,
) -> String {
    format!("{}/proposals/{}", actor_id, chain_id)
}

pub fn local_object_id(instance_uri: &str, internal_object_id: Uuid) -> String {
    format!("{}/objects/{}", instance_uri, internal_object_id)
}

pub fn local_object_id_unified(authority: &Authority, internal_object_id: Uuid) -> String {
    format!("{}/objects/{}", authority, internal_object_id)
}

pub fn local_object_replies(object_id: &str) -> String {
    format!("{}/replies", object_id)
}

pub fn local_emoji_id(instance_uri: &str, emoji_name: &str) -> String {
    format!("{}/objects/emojis/{}", instance_uri, emoji_name)
}

pub fn local_agreement_id(instance_uri: &str, invoice_id: Uuid) -> String {
    format!("{}/objects/agreements/{}", instance_uri, invoice_id)
}

// This URI redirects to the web client (it is not an actual collection)
pub fn local_tag_collection(instance_uri: &str, tag_name: &str) -> String {
    format!("{}/collections/tags/{}", instance_uri, url_encode(tag_name))
}

pub fn local_conversation_collection(
    authority: &Authority,
    conversation_id: Uuid,
) -> String {
    format!("{}/collections/conversations/{}", authority, conversation_id)
}

pub fn local_conversation_history_collection(
    authority: &Authority,
    conversation_id: Uuid,
) -> String {
    format!(
        "{}/history",
        local_conversation_collection(authority, conversation_id),
    )
}

pub fn local_activity_id(
    instance_uri: &str,
    activity_type: &str,
    internal_id: Uuid,
) -> String {
    format!(
        "{}/activities/{}/{}",
        instance_uri,
        activity_type.to_lowercase(),
        internal_id,
    )
}

pub fn local_activity_id_unified(
    authority: &Authority,
    activity_type: &str,
    internal_id: Uuid,
) -> String {
    format!(
        "{}/activities/{}/{}",
        authority,
        activity_type.to_lowercase(),
        internal_id,
    )
}

#[derive(Debug, PartialEq)]
pub enum UuidOrUsername {
    Uuid(Uuid),
    Username(String),
}

pub(crate) fn parse_local_actor_id(
    authority: &Authority,
    actor_id: &str,
) -> Result<UuidOrUsername, ValidationError> {
    let (base_uri, uuid_or_username) = match authority.root() {
        AuthorityRoot::Server(_) => {
            // See also: mitra_validators::users::USERNAME_RE
            let path_re = Regex::new(r"^/users/(?P<username>[0-9A-Za-z_\-]+)$")
                .expect("regexp should be valid");
            let (base_uri, (username,)) = parse_object_id(actor_id, path_re)
                .map_err(|_| ValidationError("invalid local actor ID"))?;
            (base_uri, UuidOrUsername::Username(username))
        },
        AuthorityRoot::Key(_) => {
            let path_re = Regex::new("^/actors/(?P<uuid>[0-9a-f-]+)$")
                .expect("regexp should be valid");
            let (base_uri, (internal_actor_id,)) = parse_object_id(actor_id, path_re)
                .map_err(|_| ValidationError("invalid local actor ID"))?;
            (base_uri, UuidOrUsername::Uuid(internal_actor_id))
        },
    };
    if base_uri != authority.root().to_string() {
        return Err(ValidationError("authority mismatch"));
    };
    Ok(uuid_or_username)
}

pub(crate) fn parse_local_object_id(
    authority: &Authority,
    object_id: &str,
) -> Result<Uuid, ValidationError> {
    let path_re = Regex::new("^/objects/(?P<uuid>[0-9a-f-]+)$")
        .expect("regexp should be valid");
    let (base_uri, (internal_object_id,)) = parse_object_id(object_id, path_re)
        .map_err(|_| ValidationError("invalid local object ID"))?;
    if base_uri != authority.root().to_string() {
        return Err(ValidationError("authority mismatch"));
    };
    Ok(internal_object_id)
}

pub(crate) fn parse_local_primary_intent_id(
    instance_uri: &str,
    proposal_id: &str,
) -> Result<(String, ChainId), ValidationError> {
    // See also: mitra_validators::users::USERNAME_RE
    let path_re = Regex::new("^/users/(?P<username>[0-9a-z_]+)/proposals/(?P<chain_id>.+)#primary$")
        .expect("regexp should be valid");
    let (base_uri, (username, chain_id)) =
        parse_object_id(proposal_id, path_re)
            .map_err(|_| ValidationError("invalid local proposal ID"))?;
    if base_uri != instance_uri {
        return Err(ValidationError("instance mismatch"));
    };
    Ok((username, chain_id))
}

pub(crate) fn parse_local_activity_id(
    authority: &Authority,
    activity_id: &str,
) -> Result<Uuid, ValidationError> {
    if let Ok(internal_activity_id) = parse_local_object_id(
        authority,
        activity_id,
    ) {
        // Legacy format
        return Ok(internal_activity_id);
    };
    let path_re = Regex::new("^/activities/[a-z]+/(?P<uuid>[0-9a-f-]+)$")
        .expect("regexp should be valid");
    let (base_uri, (internal_activity_id,)) =
        parse_object_id(activity_id, path_re)
            .map_err(|_| ValidationError("invalid local activity ID"))?;
    if base_uri != authority.root().to_string() {
        return Err(ValidationError("authority mismatch"));
    };
    Ok(internal_activity_id)
}

// Returns canonical post URI
pub fn post_object_id(authority: &Authority, post: &PostDetailed) -> String {
    match post.object_id {
        Some(ref object_id) => object_id.clone(),
        None => {
            let authority = authority.and_prefer_canonical();
            local_object_id_unified(&authority, post.id)
        },
    }
}

// Returns canonical actor URI
pub fn profile_actor_id(authority: &Authority, profile: &DbActorProfile) -> String {
    match profile.actor_json {
        Some(ref actor) => actor.id.clone(),
        None => {
            let authority = authority.and_prefer_canonical();
            local_actor_id_unified(&authority, profile.id, &profile.username)
        }
    }
}

pub fn profile_actor_url(authority: &Authority, profile: &DbActorProfile) -> String {
    match profile.actor_json {
        Some(ref actor) => {
            if let Some(ref actor_url) = actor.url {
                return actor_url.clone();
            };
            if actor.is_portable() {
                // Use compatible ID as 'url'
                return expect_compatible_actor_id(actor);
            };
            actor.id.clone()
        },
        None => {
            local_actor_id_unified(authority, profile.id, &profile.username)
        },
    }
}

/// Convert canonical object ID (from database) to compatible ID,
/// to be used in object construction.
/// If object ID is an 'ap' URI, compatible ID will be based on primary gateway.
pub(crate) fn compatible_id(
    db_actor: &DbActor,
    object_id: &str,
) -> Result<String, DatabaseTypeError> {
    // Will return error if ID is not a valid URI
    let canonical_object_id = parse_id_from_db_lenient(object_id)?;
    // TODO: FEP-EF61: at least one gateway must be stored
    let maybe_gateway = db_actor.gateways.first()
        .map(|gateway| gateway.as_str());
    // Will return error if ID is portable and there is no gateway
    let http_uri = canonical_object_id.to_http_uri(maybe_gateway)
        .ok_or(DatabaseTypeError)?;
    Ok(http_uri)
}

fn compatible_actor_id(
    db_actor: &DbActor,
) -> Result<String, DatabaseTypeError> {
   compatible_id(db_actor, &db_actor.id)
}

pub fn expect_compatible_actor_id(actor_data: &DbActor) -> String {
    compatible_actor_id(actor_data).expect("actor ID should be valid")
}

pub fn compatible_profile_actor_id(
    authority: &Authority,
    profile: &DbActorProfile,
) -> String {
    match profile.actor_json {
        Some(ref actor) => {
            if actor.is_portable() {
                expect_compatible_actor_id(actor)
            } else {
                actor.id.clone()
            }
        },
        None => local_actor_id_unified(authority, profile.id, &profile.username),
    }
}

pub fn compatible_post_object_id(
    authority: &Authority,
    post: &PostDetailed,
) -> String {
    match post.object_id {
        Some(ref object_id) => {
            let actor_data = post.author.expect_actor_data();
            if actor_data.is_portable() {
                // Use compatible ID
                compatible_id(actor_data, object_id)
                    .expect("object ID should be valid")
            } else {
                object_id.clone()
            }
        },
        None => local_object_id_unified(authority, post.id),
    }
}

pub fn canonicalize_id(id: &str) -> Result<CanonicalUri, ValidationError> {
    let canonical_uri = CanonicalUri::parse(id)
        .map_err(|error| ValidationError(error.0))?;
    Ok(canonical_uri)
}

#[cfg(test)]
mod tests {
    use apx_sdk::{
        core::{
            crypto::eddsa::generate_weak_ed25519_key,
            url::http_uri::HttpUri,
        },
    };
    use uuid::uuid;
    use mitra_utils::id::generate_ulid;
    use super::*;

    const INSTANCE_URI: &str = "https://social.example";

    #[test]
    fn test_local_activity_id() {
        let internal_id = uuid!("cb26ed69-a6e9-47e3-8bf2-bbb26d06d1fb");
        let activity_id = local_activity_id(INSTANCE_URI, "Like", internal_id);
        assert_eq!(
            activity_id,
            "https://social.example/activities/like/cb26ed69-a6e9-47e3-8bf2-bbb26d06d1fb",
        );
    }

    #[test]
    fn test_parse_local_actor_id() {
        let authority = Authority::server_unchecked(INSTANCE_URI);
        let username = parse_local_actor_id(
            &authority,
            "https://social.example/users/test",
        ).unwrap();
        assert_eq!(username, UuidOrUsername::Username("test".to_string()));
    }

    #[test]
    fn test_parse_local_actor_id_key_authority() {
        let secret_key = generate_weak_ed25519_key();
        let server_uri = HttpUri::parse(INSTANCE_URI).unwrap();
        let authority = Authority::key_with_gateway(&secret_key, &server_uri);
        let actor_id = "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actors/cb26ed69-a6e9-47e3-8bf2-bbb26d06d1fb";
        let internal_actor_id = parse_local_actor_id(
            &authority,
            &actor_id,
        ).unwrap();
        assert_eq!(
            internal_actor_id,
            UuidOrUsername::Uuid(uuid!("cb26ed69-a6e9-47e3-8bf2-bbb26d06d1fb")),
        );
    }

    #[test]
    fn test_parse_local_actor_id_wrong_path() {
        let authority = Authority::server_unchecked(INSTANCE_URI);
        let error = parse_local_actor_id(
            &authority,
            "https://social.example/user/test",
        ).unwrap_err();
        assert_eq!(error.to_string(), "invalid local actor ID");
    }

    #[test]
    fn test_parse_local_actor_id_invalid_username() {
        let authority = Authority::server_unchecked(INSTANCE_URI);
        let error = parse_local_actor_id(
            &authority,
            "https://social.example/users/tes~t",
        ).unwrap_err();
        assert_eq!(error.to_string(), "invalid local actor ID");
    }

    #[test]
    fn test_parse_local_actor_id_followers() {
        let authority = Authority::server_unchecked(INSTANCE_URI);
        let error = parse_local_actor_id(
            &authority,
            "https://social.example/users/test/followers",
        ).unwrap_err();
        assert_eq!(error.to_string(), "invalid local actor ID");
    }

    #[test]
    fn test_parse_local_actor_id_with_fragment() {
        let authority = Authority::server_unchecked(INSTANCE_URI);
        let error = parse_local_actor_id(
            &authority,
            "https://social.example/users/test#main-key",
        ).unwrap_err();
        assert_eq!(error.to_string(), "invalid local actor ID");
    }

    #[test]
    fn test_parse_local_actor_id_invalid_instance_uri() {
        let authority = Authority::server_unchecked(INSTANCE_URI);
        let error = parse_local_actor_id(
            &authority,
            "https://example.gov/users/test",
        ).unwrap_err();
        assert_eq!(error.to_string(), "authority mismatch");
    }

    #[test]
    fn test_parse_local_object_id() {
        let authority = Authority::server_unchecked(INSTANCE_URI);
        let expected_uuid = generate_ulid();
        let object_id = format!(
            "https://social.example/objects/{}",
            expected_uuid,
        );
        let internal_object_id = parse_local_object_id(
            &authority,
            &object_id,
        ).unwrap();
        assert_eq!(internal_object_id, expected_uuid);
    }

    #[test]
    fn test_parse_local_object_id_invalid_uuid() {
        let authority = Authority::server_unchecked(INSTANCE_URI);
        let object_id = "https://social.example/objects/1234";
        let error = parse_local_object_id(
            &authority,
            object_id,
        ).unwrap_err();
        assert_eq!(error.to_string(), "invalid local object ID");
    }

    #[test]
    fn test_parse_local_object_id_key_authority() {
        let secret_key = generate_weak_ed25519_key();
        let server_uri = HttpUri::parse(INSTANCE_URI).unwrap();
        let authority = Authority::key_with_gateway(&secret_key, &server_uri);
        let object_id = "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/objects/cb26ed69-a6e9-47e3-8bf2-bbb26d06d1fb";
        let internal_object_id = parse_local_object_id(
            &authority,
            &object_id,
        ).unwrap();
        assert_eq!(internal_object_id, uuid!("cb26ed69-a6e9-47e3-8bf2-bbb26d06d1fb"));
    }

    #[test]
    fn test_parse_local_primary_intent_id() {
        let proposal_id = "https://social.example/users/test/proposals/monero:418015bb9ae982a1975da7d79277c270#primary";
        let (username, chain_id) = parse_local_primary_intent_id(
            INSTANCE_URI,
            proposal_id,
        ).unwrap();
        assert_eq!(username, "test");
        assert_eq!(chain_id, ChainId::monero_mainnet());
    }

    #[test]
    fn test_parse_local_activity_id() {
        let authority = Authority::server_unchecked(INSTANCE_URI);
        let expected_internal_id = generate_ulid();
        let activity_id =
            local_activity_id(INSTANCE_URI, "Like", expected_internal_id);
        let internal_id = parse_local_activity_id(
            &authority,
            &activity_id,
        ).unwrap();
        assert_eq!(internal_id, expected_internal_id);
    }

    #[test]
    fn test_profile_actor_url() {
        let authority = Authority::server_unchecked(INSTANCE_URI);
        let profile = DbActorProfile::local_for_test("test");
        let profile_url = profile_actor_url(&authority, &profile);
        assert_eq!(
            profile_url,
            "https://social.example/users/test",
        );
    }

    #[test]
    fn test_post_object_id_ap_uri() {
        let authority = Authority::server_unchecked(INSTANCE_URI);
        let profile = DbActorProfile::remote_for_test_with_data(
            "test",
            DbActor {
                id: "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor".to_string(),
                gateways: vec!["https://gateway.example".to_string()],
                ..Default::default()
            },
        );
        let post = PostDetailed::remote_for_test(
            &profile,
            "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/posts/1",
        );
        let object_id = post_object_id(&authority, &post);
        assert_eq!(
            object_id,
            "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/posts/1",
        );
    }

    #[test]
    fn test_compatible_post_object_id() {
        let profile = DbActorProfile::remote_for_test(
            "test",
            "https://social.example/users/1",
        );
        let post = PostDetailed::remote_for_test(
            &profile,
            "https://social.example/posts/1",
        );
        let authority = Authority::server_unchecked(INSTANCE_URI);
        let object_id = compatible_post_object_id(&authority, &post);
        assert_eq!(
            object_id,
            "https://social.example/posts/1",
        );
    }

    #[test]
    fn test_compatible_post_object_id_ap_uri() {
        let profile = DbActorProfile::remote_for_test_with_data(
            "test",
            DbActor {
                id: "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor".to_string(),
                gateways: vec!["https://gateway.example".to_string()],
                ..Default::default()
            },
        );
        let post = PostDetailed::remote_for_test(
            &profile,
            "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/posts/1",
        );
        let authority = Authority::server_unchecked(INSTANCE_URI);
        let object_id = compatible_post_object_id(&authority, &post);
        assert_eq!(
            object_id,
            "https://gateway.example/.well-known/apgateway/did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/posts/1",
        );
    }

    #[test]
    fn test_compatible_profile_actor_id() {
        let authority = Authority::server_unchecked(INSTANCE_URI);
        let profile = DbActorProfile::remote_for_test(
            "test",
            "https://social.example/users/1",
        );
        let actor_id = compatible_profile_actor_id(&authority, &profile);
        assert_eq!(
            actor_id,
            "https://social.example/users/1",
        );
    }

    #[test]
    fn test_compatible_profile_actor_id_ap_uri() {
        let authority = Authority::server_unchecked(INSTANCE_URI);
        let profile = DbActorProfile::remote_for_test_with_data(
            "test",
            DbActor {
                id: "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor".to_string(),
                gateways: vec!["https://social.example".to_string()],
                ..Default::default()
            },
        );
        let actor_id = compatible_profile_actor_id(&authority, &profile);
        assert_eq!(
            actor_id,
            "https://social.example/.well-known/apgateway/did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor",
        );
    }

    #[test]
    fn test_canonicalize_id_http() {
        let url = "https://social.example/users/alice#main-key";
        let canonical_url = canonicalize_id(url).unwrap();
        assert_eq!(canonical_url.to_string(), url);

        let url = "https://social.example";
        let canonical_url = canonicalize_id(url).unwrap();
        assert_eq!(canonical_url.to_string(), url);
    }

    #[test]
    fn test_canonicalize_id_http_idn() {
        let url = "https://δοκιμή.example/users/alice#main-key";
        let result = canonicalize_id(url);
        assert!(result.is_err()); // not a URI
    }

    #[test]
    fn test_canonicalize_id_ap() {
        let url = "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor?type=group";
        let canonical_url = canonicalize_id(url).unwrap();
        assert_eq!(
            canonical_url.to_string(),
            "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor",
        );
    }

    #[test]
    fn test_canonicalize_id_gateway() {
        let url = "https://social.example/.well-known/apgateway/did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor#main-key";
        let canonical_url = canonicalize_id(url).unwrap();
        assert_eq!(
            canonical_url.to_string(),
            "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor#main-key",
        );
    }
}
