use std::collections::HashMap;

use serde::{Serialize, Deserialize};

use super::constants::{AP_CONTEXT, AP_MEDIA_TYPE, AS_MEDIA_TYPE};

const SELF_RELATION_TYPE: &str = "self";
pub const JRD_MEDIA_TYPE: &str = "application/jrd+json";

// https://datatracker.ietf.org/doc/html/rfc7033#section-4.4.4
#[derive(Deserialize, Serialize)]
pub struct Link {
    rel: String,

    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    media_type: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    href: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    template: Option<String>,

    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    properties: HashMap<String, String>,
}

impl Link {
    pub fn new(rel: &str) -> Self {
        Self {
            rel: rel.to_string(),
            media_type: None,
            href: None,
            template: None,
            properties: Default::default(),
        }
    }

    pub fn with_media_type(mut self, media_type: &str) -> Self {
        self.media_type = Some(media_type.to_string());
        self
    }

    pub fn with_href(mut self, href: &str) -> Self {
        self.href = Some(href.to_string());
        self
    }

    pub fn with_template(mut self, template: &str) -> Self {
        self.template = Some(template.to_string());
        self
    }

    pub fn actor(actor_id: &str) -> Self {
        Self::new(SELF_RELATION_TYPE)
            .with_media_type(AP_MEDIA_TYPE)
            .with_href(actor_id)
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
        let links: Vec<_> = self.links.iter()
            .filter(|link| link.rel == SELF_RELATION_TYPE)
            .filter(|link| link.media_type.iter().any(|media_type| {
                media_type == AP_MEDIA_TYPE || media_type == AS_MEDIA_TYPE
            }))
            .collect();
        // Lemmy servers can have Group and Person actors with the same name
        // https://github.com/LemmyNet/lemmy/issues/2037
        let ap_type_property = format!("{}#type", AP_CONTEXT);
        // Choose preferred type if actor type is provided.
        let mut maybe_actor_link = links.iter()
            .find(|link| {
                let ap_type = link.properties
                    .get(&ap_type_property)
                    .map(|val| val.as_str());
                ap_type == Some(preferred_type)
            });
        // Otherwise take first "self" link
        if maybe_actor_link.is_none() {
            maybe_actor_link = links.first();
        };
        let actor_id = maybe_actor_link?.href.as_ref()?.to_string();
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
            template: None,
            properties: Default::default(),
        };
        let actor_link = Link {
            rel: "self".to_string(),
            media_type: Some("application/activity+json".to_string()),
            href: Some(actor_id.to_string()),
            template: None,
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
            template: None,
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
            template: None,
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

    #[test]
    fn test_jrd_find_actor_id_piefed() {
        let jrd_value = json!({
            "aliases": ["https://piefed.example/u/user"],
            "links": [{
                "href": "https://piefed.example/u/user",
                "rel": "http://webfinger.net/rel/profile-page",
                "type": "text/html",
            }, {
                "href": "https://piefed.example/u/user",
                "properties": {"https://www.w3.org/ns/activitystreams#type": "Person"},
                "rel": "self",
                "type": "application/activity+json",
            }],
            "subject": "acct:user@piefed.example",
        });
        let jrd: JsonResourceDescriptor =
            serde_json::from_value(jrd_value).unwrap();
        assert_eq!(
            jrd.find_actor_id("Group").unwrap(),
            "https://piefed.example/u/user",
        );
    }
}
