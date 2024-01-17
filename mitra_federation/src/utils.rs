use serde_json::{Value as JsonValue};

use mitra_utils::urls::{Position, Url, UrlError};

// AP requires actor to have inbox and outbox,
// but `outbox` property is not always present.
// https://www.w3.org/TR/activitypub/#actor-objects
pub fn is_actor(value: &JsonValue) -> bool {
    value["inbox"].as_str().is_some()
}

// Activities must have `actor` property
pub fn is_activity(value: &JsonValue) -> bool {
    // Pleroma adds 'actor' property to Note objects
    !value["actor"].is_null() && value["attributedTo"].is_null()
}

pub fn is_object(value: &JsonValue) -> bool {
    !is_actor(value) && !is_activity(value)
}

pub fn key_id_to_actor_id(key_id: &str) -> Result<String, UrlError> {
    let key_url = Url::parse(key_id)?;
    let position = if key_url.query_pairs().any(|(name, _)| name == "id") {
        // Podcast Index compat
        // Strip fragment, keep query
        Position::AfterQuery
    } else {
        // Strip fragment and query (works with most AP servers)
        Position::AfterPath
    };
    let actor_id = &key_url[..position];
    // GoToSocial compat
    let actor_id = actor_id.trim_end_matches("/main-key");
    Ok(actor_id.to_string())
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use super::*;

    #[test]
    fn test_is_actor() {
        let actor = json!({
            "id": "https://social.example/actors/1",
            "type": "Person",
            "inbox": "https://social.example/actors/1/inbox",
        });
        assert_eq!(is_actor(&actor), true);
        assert_eq!(is_activity(&actor), false);
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
        assert_eq!(is_object(&activity), false);
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
        assert_eq!(is_object(&object), true);
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
    }
}
