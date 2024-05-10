use regex::Regex;
use uuid::Uuid;

use mitra_federation::identifiers::parse_object_id;
use mitra_models::{
    posts::types::Post,
    profiles::types::{
        DbActorProfile,
        PublicKeyType,
    },
};
use mitra_utils::{
    ap_url::ApUrl,
    caip2::ChainId,
    did_key::DidKey,
    urls::url_encode,
};
use mitra_validators::errors::ValidationError;

use crate::{
    authority::Authority,
};

pub fn local_actor_id_fep_ef61_fallback(instance_url: &str, username: &str) -> String {
    let actor_id = local_actor_id(instance_url, username);
    format!("{}?fep_ef61=true", actor_id)
}

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

pub fn local_object_id(instance_url: &str, internal_object_id: &Uuid) -> String {
    format!("{}/objects/{}", instance_url, internal_object_id)
}

pub fn local_object_id_fep_ef61_fallback(
    instance_url: &str,
    internal_object_id: Uuid,
) -> String {
    let object_id = local_object_id(instance_url, &internal_object_id);
    format!("{}?fep_ef61=true", object_id)
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

pub fn parse_local_actor_id(
    instance_url: &str,
    actor_id: &str,
) -> Result<String, ValidationError> {
    // See also: mitra_validators::users::USERNAME_RE
    let path_re = Regex::new("^/users/(?P<username>[0-9a-z_]+)$")
        .expect("regexp should be valid");
    let (base_url, (username,)) = parse_object_id(actor_id, path_re)
        .map_err(|_| ValidationError("invalid local actor ID"))?;
    if base_url != instance_url {
        return Err(ValidationError("instance mismatch"));
    };
    Ok(username)
}

pub fn parse_fep_ef61_local_actor_id(
    actor_id: &str,
) -> Result<DidKey, ValidationError> {
    let ap_url: ApUrl = actor_id.parse()
        .map_err(ValidationError)?;
    let did_key = ap_url.did().as_did_key()
        .ok_or(ValidationError("unexpected DID method"))?;
    if ap_url.relative_url() != "/actor" {
        return Err(ValidationError("invalid path"));
    };
    Ok(did_key.clone())
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

pub fn parse_fep_ef61_local_object_id(
    object_id: &str,
) -> Result<(DidKey, Uuid), ValidationError> {
    let ap_url: ApUrl = object_id.parse()
        .map_err(ValidationError)?;
    let did_key = ap_url.did().as_did_key()
        .ok_or(ValidationError("unexpected DID method"))?;
    let path = ap_url.relative_url();
    let path_re = Regex::new("^/objects/(?P<uuid>[0-9a-f-]+)$")
        .expect("regexp should be valid");
    let path_caps = path_re.captures(&path)
        .ok_or(ValidationError("invalid path"))?;
    let internal_object_id = path_caps["uuid"].parse()
        .map_err(|_| ValidationError("invalid path"))?;
    Ok((did_key.clone(), internal_object_id))
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

pub fn post_object_id(instance_url: &str, post: &Post) -> String {
    match post.object_id {
        Some(ref object_id) => object_id.to_string(),
        None => local_object_id(instance_url, &post.id),
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
            return actor_url.to_string();
        };
    };
    profile_actor_id(instance_url, profile)
}

#[cfg(test)]
mod tests {
    use uuid::uuid;
    use mitra_utils::id::generate_ulid;
    use super::*;

    const INSTANCE_URL: &str = "https://social.example";

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
            "https://social.example/users/tes-t",
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
    fn test_parse_fep_ef61_local_actor_id() {
        let actor_id = "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor";
        let did_key = parse_fep_ef61_local_actor_id(actor_id).unwrap();
        assert_eq!(
            did_key.to_string(),
            "did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6",
        );
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
    fn test_parse_fep_ef61_local_object_id() {
        let object_id = "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/objects/cb26ed69-a6e9-47e3-8bf2-bbb26d06d1fb";
        let (did_key, internal_object_id) =
            parse_fep_ef61_local_object_id(object_id).unwrap();
        assert_eq!(
            did_key.to_string(),
            "did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6",
        );
        assert_eq!(
            internal_object_id,
            uuid!("cb26ed69-a6e9-47e3-8bf2-bbb26d06d1fb"),
        );
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
    fn test_profile_actor_url() {
        let profile = DbActorProfile {
            username: "test".to_string(),
            ..Default::default()
        };
        let profile_url = profile_actor_url(INSTANCE_URL, &profile);
        assert_eq!(
            profile_url,
            "https://social.example/users/test",
        );
    }
}
