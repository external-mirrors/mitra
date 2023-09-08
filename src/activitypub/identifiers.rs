use regex::Regex;
use url::Url;
use uuid::Uuid;

use mitra_models::{
    posts::types::Post,
    profiles::types::{
        DbActorProfile,
        PublicKeyType,
    },
};
use mitra_utils::{
    caip2::ChainId,
    urls::{get_hostname, url_encode},
};
use mitra_validators::errors::ValidationError;

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

pub fn local_actor_inbox(instance_url: &str, username: &str) -> String {
    let actor_id = local_actor_id(instance_url, username);
    LocalActorCollection::Inbox.of(&actor_id)
}

pub fn local_actor_outbox(instance_url: &str, username: &str) -> String {
    let actor_id = local_actor_id(instance_url, username);
    LocalActorCollection::Outbox.of(&actor_id)
}

pub fn local_actor_followers(instance_url: &str, username: &str) -> String {
    let actor_id = local_actor_id(instance_url, username);
    LocalActorCollection::Followers.of(&actor_id)
}

pub fn local_actor_following(instance_url: &str, username: &str) -> String {
    let actor_id = local_actor_id(instance_url, username);
    LocalActorCollection::Following.of(&actor_id)
}

pub fn local_actor_subscribers(instance_url: &str, username: &str) -> String {
    let actor_id = local_actor_id(instance_url, username);
    LocalActorCollection::Subscribers.of(&actor_id)
}

pub fn local_actor_featured(instance_url: &str, username: &str) -> String {
    let actor_id = local_actor_id(instance_url, username);
    LocalActorCollection::Featured.of(&actor_id)
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
    instance_url: &str,
    username: &str,
    chain_id: &ChainId,
) -> String {
    let actor_id = local_actor_id(instance_url, username);
    format!("{}/proposals/{}", actor_id, chain_id)
}

pub fn local_object_id(instance_url: &str, internal_object_id: &Uuid) -> String {
    format!("{}/objects/{}", instance_url, internal_object_id)
}

pub fn local_emoji_id(instance_url: &str, emoji_name: &str) -> String {
    format!("{}/objects/emojis/{}", instance_url, emoji_name)
}

pub fn local_agreement_id(instance_url: &str, invoice_id: &Uuid) -> String {
    format!("{}/objects/agreements/{}", instance_url, invoice_id)
}

pub fn local_tag_collection(instance_url: &str, tag_name: &str) -> String {
    format!("{}/collections/tags/{}", instance_url, url_encode(tag_name))
}

pub fn validate_object_id(object_id: &str) -> Result<(), ValidationError> {
    get_hostname(object_id)
        .map_err(|_| ValidationError("invalid object ID"))?;
    Ok(())
}

pub fn parse_local_actor_id(
    instance_url: &str,
    actor_id: &str,
) -> Result<String, ValidationError> {
    let url = Url::parse(actor_id)
        .map_err(|_| ValidationError("invalid URL"))?;
    if url.origin().unicode_serialization() != instance_url {
        return Err(ValidationError("instance mismatch"));
    };
    // See also: mitra_validators::users::USERNAME_RE
    let url_regexp = Regex::new("^/users/(?P<username>[0-9a-z_]+)$")
        .expect("regexp should be valid");
    let url_caps = url_regexp.captures(url.path())
        .ok_or(ValidationError("invalid actor ID"))?;
    let username = url_caps.name("username")
        .ok_or(ValidationError("invalid actor ID"))?
        .as_str()
        .to_owned();
    Ok(username)
}

pub fn parse_local_object_id(
    instance_url: &str,
    object_id: &str,
) -> Result<Uuid, ValidationError> {
    let url = Url::parse(object_id)
        .map_err(|_| ValidationError("invalid URL"))?;
    if url.origin().unicode_serialization() != instance_url {
        return Err(ValidationError("instance mismatch"));
    };
    let url_regexp = Regex::new("^/objects/(?P<uuid>[0-9a-f-]+)$")
        .expect("regexp should be valid");
    let url_caps = url_regexp.captures(url.path())
        .ok_or(ValidationError("invalid object ID"))?;
    let internal_object_id: Uuid = url_caps.name("uuid")
        .ok_or(ValidationError("invalid object ID"))?
        .as_str().parse()
        .map_err(|_| ValidationError("invalid object ID"))?;
    Ok(internal_object_id)
}

// Works with fragment-based intent IDs too
pub fn parse_local_proposal_id(
    instance_url: &str,
    proposal_id: &str,
) -> Result<(String, ChainId), ValidationError> {
    let url = Url::parse(proposal_id)
        .map_err(|_| ValidationError("invalid URL"))?;
    if url.origin().unicode_serialization() != instance_url {
        return Err(ValidationError("instance mismatch"));
    };
    // See also: mitra_validators::users::USERNAME_RE
    let url_regexp = Regex::new("^/users/(?P<username>[0-9a-z_]+)/proposals/(?P<chain_id>.+)$")
        .expect("regexp should be valid");
    let url_caps = url_regexp.captures(url.path())
        .ok_or(ValidationError("invalid proposal ID"))?;
    let username = url_caps.name("username")
        .ok_or(ValidationError("invalid proposal ID"))?
        .as_str()
        .to_owned();
    let chain_id = url_caps.name("chain_id")
        .ok_or(ValidationError("invalid proposal ID"))?
        .as_str()
        .parse()
        .map_err(|_| ValidationError("invalid chain ID"))?;
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
        assert_eq!(error.to_string(), "invalid actor ID");
    }

    #[test]
    fn test_parse_local_actor_id_invalid_username() {
        let error = parse_local_actor_id(
            INSTANCE_URL,
            "https://social.example/users/tes-t",
        ).unwrap_err();
        assert_eq!(error.to_string(), "invalid actor ID");
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
        assert_eq!(error.to_string(), "invalid object ID");
    }

    #[test]
    fn test_parse_local_proposal_id() {
        let proposal_id = "https://social.example/users/test/proposals/monero:418015bb9ae982a1975da7d79277c270";
        let (username, chain_id) = parse_local_proposal_id(
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
