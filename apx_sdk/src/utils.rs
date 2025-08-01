//! Miscellaneous utilities.

use serde_json::{Value as JsonValue};

use apx_core::{
    http_types::HeaderValue,
    http_url::HttpUrl,
    http_utils::remove_quotes,
};

use super::constants::AP_PUBLIC;

/// Core object type
pub enum CoreType {
    Object,
    Link,
    Actor,
    Activity,
    Collection,
    VerificationMethod,
}

/// Determines the core type of an object.
pub fn get_core_type(value: &JsonValue) -> CoreType {
    // https://codeberg.org/fediverse/fep/src/branch/main/fep/2277/fep-2277.md
    if
        !value["publicKeyPem"].is_null() ||
        !value["publicKeyMultibase"].is_null()
    {
        CoreType::VerificationMethod
    }
    else if !value["href"].is_null() {
        // `href` may only appear in Link objects:
        // https://www.w3.org/TR/activitystreams-vocabulary/#dfn-href
        CoreType::Link
    }
    else if !value["inbox"].is_null() {
        // AP requires actor to have inbox and outbox,
        // but `outbox` property is not always present.
        // https://www.w3.org/TR/activitypub/#actor-objects
        CoreType::Actor
    }
    else if !value["actor"].is_null() && value["attributedTo"].is_null() {
        // Activities must have an `actor` property:
        // https://www.w3.org/TR/activitystreams-vocabulary/#dfn-actor
        // However, Pleroma adds 'actor' property to Note objects
        // https://git.pleroma.social/pleroma/pleroma/-/issues/3269
        // https://akkoma.dev/AkkomaGang/akkoma/issues/770
        CoreType::Activity
    }
    else if
        !value["items"].is_null() ||
        !value["orderedItems"].is_null() ||
        !value["totalItems"].is_null() ||
        !value["partOf"].is_null() ||
        !value["first"].is_null() ||
        !value["last"].is_null() ||
        !value["next"].is_null() ||
        !value["prev"].is_null() ||
        !value["current"].is_null()
    {
        // `items` may only appear in Collection objects:
        // https://www.w3.org/TR/activitystreams-vocabulary/#dfn-items
        CoreType::Collection
    }
    else {
        CoreType::Object
    }
}

pub fn is_verification_method(value: &JsonValue) -> bool {
    matches!(get_core_type(value), CoreType::VerificationMethod)
}

pub fn is_actor(value: &JsonValue) -> bool {
    matches!(get_core_type(value), CoreType::Actor)
}

pub fn is_activity(value: &JsonValue) -> bool {
    matches!(get_core_type(value), CoreType::Activity)
}

pub fn is_collection(value: &JsonValue) -> bool {
    matches!(get_core_type(value), CoreType::Collection)
}

pub fn is_object(value: &JsonValue) -> bool {
    matches!(get_core_type(value), CoreType::Object | CoreType::Link)
}

pub fn key_id_to_actor_id(key_id: &str) -> Result<String, &'static str> {
    let key_url = HttpUrl::parse(key_id)?;
    let actor_id = if key_url.query().filter(|query| query.contains("id=")).is_some() {
        // Podcast Index compat
        // Strip fragment, keep query
        key_url.without_fragment()
    } else {
        // Strip fragment and query (works with most AP servers)
        key_url.without_query_and_fragment()
    };
    // GoToSocial compat
    let actor_id = actor_id.trim_end_matches("/main-key");
    Ok(actor_id.to_string())
}

/// Returns `true` if the given string is a representation of the `Public` collection
pub fn is_public(target_id: impl AsRef<str>) -> bool {
    // Some servers use "as" namespace
    // https://www.w3.org/TR/activitypub/#public-addressing
    const PUBLIC_VARIANTS: [&str; 3] = [
        AP_PUBLIC,
        "as:Public",
        "Public",
    ];
    PUBLIC_VARIANTS.contains(&target_id.as_ref())
}

/// Extract media type from Content-Type or Accept header
pub fn extract_media_type(header_value: &HeaderValue) -> Option<String> {
    header_value.to_str().ok()
        // Take first media type if there are many
        .and_then(|value| value.split(',').next())
        // Normalize
        // https://httpwg.org/specs/rfc9110.html#media.type
        .map(|value| {
            value
                .split(';')
                .map(|part| {
                    let part = part.trim();
                    if let Some((key, value)) = part.split_once('=') {
                        let value = remove_quotes(value);
                        format!(r#"{key}="{value}""#)
                    } else {
                        part.to_string()
                    }
                })
                // Remove 'q' and 'charset' directives
                .filter(|part| !part.starts_with("q=") && !part.starts_with("charset="))
                .collect::<Vec<_>>()
                .join("; ")
        })
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use super::*;

    #[test]
    fn test_get_core_type_verification_method() {
        let public_key = json!({
            "id": "https://social/example/actors/1#main-key",
            "owner": "https://social.example/actors/1",
            "publicKeyPem": "-----BEGIN PUBLIC KEY-----\ntest\n-----END PUBLIC KEY-----\n\n",
        });
        let core_type = get_core_type(&public_key);
        assert!(matches!(core_type, CoreType::VerificationMethod));
    }

    #[test]
    fn test_is_actor() {
        let actor = json!({
            "id": "https://social.example/actors/1",
            "type": "Person",
            "inbox": "https://social.example/actors/1/inbox",
        });
        assert_eq!(is_actor(&actor), true);
        assert_eq!(is_activity(&actor), false);
        assert_eq!(is_collection(&actor), false);
        assert_eq!(is_object(&actor), false);
    }

    #[test]
    fn test_is_activity() {
        let activity = json!({
            "id": "https://social.example/activities/1",
            "type": "Follow",
            "actor": "https://social.example/actors/1",
            "object": "https:/other.example/actors/abc",
        });
        assert_eq!(is_actor(&activity), false);
        assert_eq!(is_activity(&activity), true);
        assert_eq!(is_collection(&activity), false);
        assert_eq!(is_object(&activity), false);
    }

    #[test]
    fn test_is_collection() {
        let collection = json!({
            "id": "https://social.example/collection/1",
            "type": "Collection",
            "items": ["https://social.example/objects/1"],
        });
        assert_eq!(is_actor(&collection), false);
        assert_eq!(is_activity(&collection), false);
        assert_eq!(is_collection(&collection), true);
        assert_eq!(is_object(&collection), false);
    }

    #[test]
    fn test_is_object() {
        let object = json!({
            "id": "https://social.example/objects/1",
            "type": "Note",
            "actor": "https://social.example/actors/1",
            "attributedTo": "https://social.example/actors/1",
            "content": "test",
        });
        assert_eq!(is_actor(&object), false);
        assert_eq!(is_activity(&object), false);
        assert_eq!(is_collection(&object), false);
        assert_eq!(is_object(&object), true);
    }

    #[test]
    fn test_is_object_lemmy_group() {
        let actor = json!({
            "id": "https://group.example/c/test",
            "type": "Group",
            "attributedTo": ["https://group.example/u/mod"],
            "inbox": "https://group.example/c/test/inbox",
            "outbox": "https://group.example/c/test/outbox",
        });
        assert_eq!(is_actor(&actor), true);
        assert_eq!(is_object(&actor), false);
    }

    #[test]
    fn test_key_id_to_actor_id() {
        let key_id = "https://server.example/actor#main-key";
        let actor_id = key_id_to_actor_id(key_id).unwrap();
        assert_eq!(actor_id, "https://server.example/actor");

        // Streams
        let key_id = "https://fediversity.site/channel/mikedev?operation=rsakey";
        let actor_id = key_id_to_actor_id(key_id).unwrap();
        assert_eq!(actor_id, "https://fediversity.site/channel/mikedev");

        // GoToSocial
        let key_id = "https://myserver.org/actor/main-key";
        let actor_id = key_id_to_actor_id(key_id).unwrap();
        assert_eq!(actor_id, "https://myserver.org/actor");

        // Podcast Index
        let key_id = "https://ap.podcastindex.org/podcasts?id=920666#main-key";
        let actor_id = key_id_to_actor_id(key_id).unwrap();
        assert_eq!(actor_id, "https://ap.podcastindex.org/podcasts?id=920666");

        // microblog.pub
        let key_id = "https://social.example#main-key";
        let actor_id = key_id_to_actor_id(key_id).unwrap();
        assert_eq!(actor_id, "https://social.example");
    }

    #[test]
    fn test_extract_media_type_no_whitespace() {
        let header_value = HeaderValue::from_static(r#"application/ld+json;profile="https://www.w3.org/ns/activitystreams""#);
        let media_type = extract_media_type(&header_value).unwrap();
        assert_eq!(media_type, r#"application/ld+json; profile="https://www.w3.org/ns/activitystreams""#);
    }

    #[test]
    fn test_extract_media_type_with_charset() {
        let header_value = HeaderValue::from_static(r#"application/ld+json; profile="https://www.w3.org/ns/activitystreams"; charset=utf-8"#);
        let media_type = extract_media_type(&header_value).unwrap();
        assert_eq!(media_type, r#"application/ld+json; profile="https://www.w3.org/ns/activitystreams""#);
    }

    #[test]
    fn test_extract_media_type_profile_unquoted() {
        let header_value = HeaderValue::from_static(r#"application/ld+json; profile=https://www.w3.org/ns/activitystreams"#);
        let media_type = extract_media_type(&header_value).unwrap();
        assert_eq!(media_type, r#"application/ld+json; profile="https://www.w3.org/ns/activitystreams""#);
    }
}
