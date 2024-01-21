use std::collections::HashMap;

use serde::{Serialize, Deserialize};

use super::constants::{AP_CONTEXT, AP_MEDIA_TYPE};

const SELF_RELATION_TYPE: &str = "self";
pub const JRD_MEDIA_TYPE: &str = "application/jrd+json";

// https://datatracker.ietf.org/doc/html/rfc7033#section-4.4.4
#[derive(Deserialize, Serialize)]
pub struct Link {
    pub rel: String,

    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,

    pub href: Option<String>,

    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub properties: HashMap<String, String>,
}

impl Link {
    pub fn actor(actor_id: &str) -> Self {
        Self {
            rel: SELF_RELATION_TYPE.to_string(),
            href: Some(actor_id.to_string()),
            media_type: Some(AP_MEDIA_TYPE.to_string()),
            properties: Default::default(),
        }
    }
}

// https://datatracker.ietf.org/doc/html/rfc7033#section-4.4
#[derive(Deserialize, Serialize)]
pub struct JsonResourceDescriptor {
    pub subject: String,
    pub links: Vec<Link>,
}

impl JsonResourceDescriptor {
    pub fn find_actor_id(&self, preferred_type: &str) -> Option<String> {
        // Lemmy servers can have Group and Person actors with the same name
        // https://github.com/LemmyNet/lemmy/issues/2037
        let ap_type_property = format!("{}#type", AP_CONTEXT);
        let link = self.links.iter()
            .filter(|link| link.rel == SELF_RELATION_TYPE)
            .find(|link| {
                let ap_type = link.properties
                    .get(&ap_type_property)
                    .map(|val| val.as_str());
                // Choose preferred type if actor type is provided.
                // Otherwise take first "self" link
                ap_type.is_none() || ap_type == Some(preferred_type)
            })?;
        let actor_id = link.href.as_ref()?.to_string();
        Some(actor_id)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use super::*;

    #[test]
    fn test_create_actor_link() {
        let actor_id = "https://social.example/users/test";
        let link = Link::actor(actor_id);
        let link_value = serde_json::to_value(link).unwrap();
        assert_eq!(
            link_value,
            json!({
                "rel": "self",
                "type": AP_MEDIA_TYPE,
                "href": actor_id,
            }),
        );
    }

    #[test]
    fn test_jrd_find_actor_id() {
        let actor_id = "https://social.example/users/test";
        let profile_link = Link {
            rel: "http://webfinger.net/rel/profile-page".to_string(),
            media_type: Some("text/html".to_string()),
            href: Some(actor_id.to_string()),
            properties: Default::default(),
        };
        let actor_link = Link {
            rel: "self".to_string(),
            media_type: Some("application/activity+json".to_string()),
            href: Some(actor_id.to_string()),
            properties: Default::default(),
        };
        let jrd = JsonResourceDescriptor {
            subject: "acct:test@social.example".to_string(),
            links: vec![profile_link, actor_link],
        };
        assert_eq!(jrd.find_actor_id("Service").unwrap(), actor_id);
    }

    #[test]
    fn test_jrd_find_actor_id_lemmy() {
        let person_id = "https://lemmy.example/u/test";
        let person_link = Link {
            rel: "self".to_string(),
            media_type: Some("application/activity+json".to_string()),
            href: Some(person_id.to_string()),
            properties: HashMap::from([(
                "https://www.w3.org/ns/activitystreams#type".to_string(),
                "Person".to_string(),
            )]),
        };
        let group_id = "https://lemmy.example/c/test";
        let group_link = Link {
            rel: "self".to_string(),
            media_type: Some("application/activity+json".to_string()),
            href: Some(group_id.to_string()),
            properties: HashMap::from([(
                "https://www.w3.org/ns/activitystreams#type".to_string(),
                "Group".to_string(),
            )]),
        };
        let jrd = JsonResourceDescriptor {
            subject: "acct:test@social.example".to_string(),
            links: vec![person_link, group_link],
        };
        assert_eq!(jrd.find_actor_id("Group").unwrap(), group_id);
    }
}
