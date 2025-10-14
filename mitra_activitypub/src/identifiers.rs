use apx_core::{
    caip2::ChainId,
    url::{
        canonical::{parse_url, CanonicalUri},
        common::url_encode,
    },
};
use apx_sdk::identifiers::parse_object_id;
use regex::Regex;
use uuid::Uuid;

use mitra_models::{
    database::DatabaseTypeError,
    posts::types::Post,
    profiles::types::{
        DbActor,
        DbActorProfile,
        PublicKeyType,
    },
};
use mitra_validators::errors::ValidationError;

use crate::authority::Authority;

pub fn local_actor_id_unified(authority: &Authority, username: &str) -> String {
    match authority {
        Authority::Server(_) => local_actor_id(&authority.to_string(), username),
        Authority::Key(_) => local_instance_actor_id(&authority.to_string()),
        Authority::KeyWithGateway(_) => local_instance_actor_id(&authority.to_string()),
    }
}

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
pub fn local_actor_id(instance_url: &str, username: &str) -> String {
    format!("{}/users/{}", instance_url, username)
}

pub fn local_instance_actor_id(instance_url: &str) -> String {
    format!("{}/actor", instance_url)
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

pub fn local_object_id(instance_url: &str, internal_object_id: Uuid) -> String {
    format!("{}/objects/{}", instance_url, internal_object_id)
}

pub fn local_object_id_unified(authority: &Authority, internal_object_id: Uuid) -> String {
    format!("{}/objects/{}", authority, internal_object_id)
}

pub fn local_object_replies(object_id: &str) -> String {
    format!("{}/replies", object_id)
}

pub fn local_emoji_id(instance_url: &str, emoji_name: &str) -> String {
    format!("{}/objects/emojis/{}", instance_url, emoji_name)
}

pub fn local_agreement_id(instance_url: &str, invoice_id: Uuid) -> String {
    format!("{}/objects/agreements/{}", instance_url, invoice_id)
}

pub fn local_tag_collection(instance_url: &str, tag_name: &str) -> String {
    format!("{}/collections/tags/{}", instance_url, url_encode(tag_name))
}

pub fn local_conversation_collection(instance_url: &str, conversation_id: Uuid) -> String {
    format!("{}/collections/conversations/{}", instance_url, conversation_id)
}

pub fn local_conversation_history_collection(
    instance_url: &str,
    conversation_id: Uuid,
) -> String {
    format!(
        "{}/history",
        local_conversation_collection(instance_url, conversation_id),
    )
}

pub fn local_activity_id(
    instance_url: &str,
    activity_type: &str,
    internal_id: Uuid,
) -> String {
    format!(
        "{}/activities/{}/{}",
        instance_url,
        activity_type.to_lowercase(),
        internal_id,
    )
}

pub fn parse_local_actor_id(
    instance_url: &str,
    actor_id: &str,
) -> Result<String, ValidationError> {
    // See also: mitra_validators::users::USERNAME_RE
    let path_re = Regex::new(r"^/users/(?P<username>[0-9A-Za-z_\-]+)$")
        .expect("regexp should be valid");
    let (base_url, (username,)) = parse_object_id(actor_id, path_re)
        .map_err(|_| ValidationError("invalid local actor ID"))?;
    if base_url != instance_url {
        return Err(ValidationError("instance mismatch"));
    };
    Ok(username)
}

pub fn parse_local_object_id(
    instance_url: &str,
    object_id: &str,
) -> Result<Uuid, ValidationError> {
    let path_re = Regex::new("^/objects/(?P<uuid>[0-9a-f-]+)$")
        .expect("regexp should be valid");
    let (base_url, (internal_object_id,)) = parse_object_id(object_id, path_re)
        .map_err(|_| ValidationError("invalid local object ID"))?;
    if base_url != instance_url {
        return Err(ValidationError("instance mismatch"));
    };
    Ok(internal_object_id)
}

pub fn parse_local_primary_intent_id(
    instance_url: &str,
    proposal_id: &str,
) -> Result<(String, ChainId), ValidationError> {
    // See also: mitra_validators::users::USERNAME_RE
    let path_re = Regex::new("^/users/(?P<username>[0-9a-z_]+)/proposals/(?P<chain_id>.+)#primary$")
        .expect("regexp should be valid");
    let (base_url, (username, chain_id)) =
        parse_object_id(proposal_id, path_re)
            .map_err(|_| ValidationError("invalid local proposal ID"))?;
    if base_url != instance_url {
        return Err(ValidationError("instance mismatch"));
    };
    Ok((username, chain_id))
}

pub fn parse_local_activity_id(
    instance_url: &str,
    activity_id: &str,
) -> Result<Uuid, ValidationError> {
    if let Ok(internal_activity_id) = parse_local_object_id(
        instance_url,
        activity_id,
    ) {
        // Legacy format
        return Ok(internal_activity_id);
    };
    let path_re = Regex::new("^/activities/[a-z]+/(?P<uuid>[0-9a-f-]+)$")
        .expect("regexp should be valid");
    let (base_url, (internal_activity_id,)) =
        parse_object_id(activity_id, path_re)
            .map_err(|_| ValidationError("invalid local activity ID"))?;
    if base_url != instance_url {
        return Err(ValidationError("instance mismatch"));
    };
    Ok(internal_activity_id)
}

pub fn post_object_id(instance_url: &str, post: &Post) -> String {
    match post.object_id {
        Some(ref object_id) => object_id.clone(),
        None => local_object_id(instance_url, post.id),
    }
}

pub fn profile_actor_id(instance_url: &str, profile: &DbActorProfile) -> String {
    match profile.actor_json {
        Some(ref actor) => actor.id.clone(),
        None => local_actor_id(instance_url, &profile.username),
    }
}

pub fn profile_actor_url(instance_url: &str, profile: &DbActorProfile) -> String {
    if let Some(ref actor) = profile.actor_json {
        if let Some(ref actor_url) = actor.url {
            return actor_url.clone();
        };
        if actor.is_portable() {
            // Use compatible ID as 'url'
            return compatible_actor_id(actor)
                .expect("actor ID should be valid");
        };
    };
    profile_actor_id(instance_url, profile)
}

/// Convert canonical object ID (from database) to compatible ID,
/// to be used in object construction.
/// If object ID is an 'ap' URL, compatible ID will be based on primary gateway.
pub fn compatible_id(
    db_actor: &DbActor,
    object_id: &str,
) -> Result<String, DatabaseTypeError> {
    // ID is expected to be valid
    let (canonical_object_id, maybe_gateway) = parse_url(object_id)
        .map_err(|_| DatabaseTypeError)?;
    if maybe_gateway.is_some() {
        // Compatible IDs can't be stored
        return Err(DatabaseTypeError);
    };
    // TODO: FEP-EF61: at least one gateway must be stored
    let maybe_gateway = db_actor.gateways.first()
        .map(|gateway| gateway.as_str());
    let http_uri = canonical_object_id.to_http_uri(maybe_gateway)
        .ok_or(DatabaseTypeError)?;
    Ok(http_uri)
}

pub fn compatible_actor_id(
    db_actor: &DbActor,
) -> Result<String, DatabaseTypeError> {
   compatible_id(db_actor, &db_actor.id)
}

pub fn compatible_profile_actor_id(
    instance_url: &str,
    profile: &DbActorProfile,
) -> String {
    match profile.actor_json {
        Some(ref actor) => {
            if actor.is_portable() {
                compatible_actor_id(actor)
                    .expect("actor ID should be valid")
            } else {
                actor.id.clone()
            }
        },
        None => local_actor_id(instance_url, &profile.username),
    }
}

pub fn compatible_post_object_id(instance_url: &str, post: &Post) -> String {
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
        None => local_object_id(instance_url, post.id),
    }
}

pub fn canonicalize_id(id: &str) -> Result<CanonicalUri, ValidationError> {
    let canonical_uri = CanonicalUri::parse(id)
        .map_err(|error| ValidationError(error.0))?;
    Ok(canonical_uri)
}

#[cfg(test)]
mod tests {
    use uuid::uuid;
    use mitra_utils::id::generate_ulid;
    use super::*;

    const INSTANCE_URL: &str = "https://social.example";

    #[test]
    fn test_local_activity_id() {
        let internal_id = uuid!("cb26ed69-a6e9-47e3-8bf2-bbb26d06d1fb");
        let activity_id = local_activity_id(INSTANCE_URL, "Like", internal_id);
        assert_eq!(
            activity_id,
            "https://social.example/activities/like/cb26ed69-a6e9-47e3-8bf2-bbb26d06d1fb",
        );
    }

    #[test]
    fn test_parse_local_actor_id() {
        let username = parse_local_actor_id(
            INSTANCE_URL,
            "https://social.example/users/test",
        ).unwrap();
        assert_eq!(username, "test".to_string());
    }

    #[test]
    fn test_parse_local_actor_id_wrong_path() {
        let error = parse_local_actor_id(
            INSTANCE_URL,
            "https://social.example/user/test",
        ).unwrap_err();
        assert_eq!(error.to_string(), "invalid local actor ID");
    }

    #[test]
    fn test_parse_local_actor_id_invalid_username() {
        let error = parse_local_actor_id(
            INSTANCE_URL,
            "https://social.example/users/tes~t",
        ).unwrap_err();
        assert_eq!(error.to_string(), "invalid local actor ID");
    }

    #[test]
    fn test_parse_local_actor_id_followers() {
        let error = parse_local_actor_id(
            INSTANCE_URL,
            "https://social.example/users/test/followers",
        ).unwrap_err();
        assert_eq!(error.to_string(), "invalid local actor ID");
    }

    #[test]
    fn test_parse_local_actor_id_with_fragment() {
        let error = parse_local_actor_id(
            INSTANCE_URL,
            "https://social.example/users/test#main-key",
        ).unwrap_err();
        assert_eq!(error.to_string(), "invalid local actor ID");
    }

    #[test]
    fn test_parse_local_actor_id_invalid_instance_url() {
        let error = parse_local_actor_id(
            INSTANCE_URL,
            "https://example.gov/users/test",
        ).unwrap_err();
        assert_eq!(error.to_string(), "instance mismatch");
    }

    #[test]
    fn test_parse_local_object_id() {
        let expected_uuid = generate_ulid();
        let object_id = format!(
            "https://social.example/objects/{}",
            expected_uuid,
        );
        let internal_object_id = parse_local_object_id(
            INSTANCE_URL,
            &object_id,
        ).unwrap();
        assert_eq!(internal_object_id, expected_uuid);
    }

    #[test]
    fn test_parse_local_object_id_invalid_uuid() {
        let object_id = "https://social.example/objects/1234";
        let error = parse_local_object_id(
            INSTANCE_URL,
            object_id,
        ).unwrap_err();
        assert_eq!(error.to_string(), "invalid local object ID");
    }

    #[test]
    fn test_parse_local_primary_intent_id() {
        let proposal_id = "https://social.example/users/test/proposals/monero:418015bb9ae982a1975da7d79277c270#primary";
        let (username, chain_id) = parse_local_primary_intent_id(
            INSTANCE_URL,
            proposal_id,
        ).unwrap();
        assert_eq!(username, "test");
        assert_eq!(chain_id, ChainId::monero_mainnet());
    }

    #[test]
    fn test_parse_local_activity_id() {
        let expected_internal_id = generate_ulid();
        let activity_id =
            local_activity_id(INSTANCE_URL, "Like", expected_internal_id);
        let internal_id = parse_local_activity_id(
            INSTANCE_URL,
            &activity_id,
        ).unwrap();
        assert_eq!(internal_id, expected_internal_id);
    }

    #[test]
    fn test_profile_actor_url() {
        let profile = DbActorProfile::local_for_test("test");
        let profile_url = profile_actor_url(INSTANCE_URL, &profile);
        assert_eq!(
            profile_url,
            "https://social.example/users/test",
        );
    }

    #[test]
    fn test_compatible_post_object_id() {
        let profile = DbActorProfile::remote_for_test(
            "test",
            "https://social.example/users/1",
        );
        let post = Post::remote_for_test(
            &profile,
            "https://social.example/posts/1",
        );
        let object_id = compatible_post_object_id(INSTANCE_URL, &post);
        assert_eq!(
            object_id,
            "https://social.example/posts/1",
        );
    }

    #[test]
    fn test_compatible_post_object_id_ap_url() {
        let profile = DbActorProfile::remote_for_test_with_data(
            "test",
            DbActor {
                id: "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor".to_string(),
                gateways: vec!["https://social.example".to_string()],
                ..Default::default()
            },
        );
        let post = Post::remote_for_test(
            &profile,
            "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/posts/1",
        );
        let object_id = compatible_post_object_id(INSTANCE_URL, &post);
        assert_eq!(
            object_id,
            "https://social.example/.well-known/apgateway/did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/posts/1",
        );
    }

    #[test]
    fn test_compatible_profile_actor_id() {
        let profile = DbActorProfile::remote_for_test(
            "test",
            "https://social.example/users/1",
        );
        let actor_id = compatible_profile_actor_id(INSTANCE_URL, &profile);
        assert_eq!(
            actor_id,
            "https://social.example/users/1",
        );
    }

    #[test]
    fn test_compatible_profile_actor_id_ap_url() {
        let profile = DbActorProfile::remote_for_test_with_data(
            "test",
            DbActor {
                id: "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor".to_string(),
                gateways: vec!["https://social.example".to_string()],
                ..Default::default()
            },
        );
        let actor_id = compatible_profile_actor_id(INSTANCE_URL, &profile);
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
        assert_eq!(canonical_url.to_string(), url);
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
