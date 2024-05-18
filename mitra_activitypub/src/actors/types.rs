use serde::{
    Deserialize,
    Deserializer,
    Serialize,
    de::{Error as DeserializerError},
};
use serde_json::{Value as JsonValue};

use mitra_federation::{
    addresses::ActorAddress,
    deserialization::{
        deserialize_object_array,
        parse_into_array,
        parse_into_href_array,
    },
};
use mitra_models::{
    profiles::types::DbActor,
};
use mitra_utils::urls::get_hostname;
use mitra_validators::errors::ValidationError;

use super::keys::{Multikey, PublicKey};

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActorImage {
    #[serde(rename = "type")]
    pub object_type: String,
    pub url: String,
    pub media_type: Option<String>,
}

fn deserialize_image_opt<'de, D>(
    deserializer: D,
) -> Result<Option<ActorImage>, D::Error>
    where D: Deserializer<'de>
{
    let maybe_value: Option<JsonValue> = Option::deserialize(deserializer)?;
    let maybe_image = if let Some(value) = maybe_value {
        // Some implementations use empty object instead of null
        let is_empty_object = value.as_object()
            .map(|map| map.is_empty())
            .unwrap_or(false);
        if is_empty_object {
            None
        } else {
            let images: Vec<ActorImage> = parse_into_array(&value)
                .map_err(DeserializerError::custom)?;
            // Take first image
            images.into_iter().next()
        }
    } else {
        None
    };
    Ok(maybe_image)
}

fn deserialize_url_opt<'de, D>(
    deserializer: D,
) -> Result<Option<String>, D::Error>
    where D: Deserializer<'de>
{
    let maybe_value: Option<JsonValue> = Option::deserialize(deserializer)?;
    let maybe_url = if let Some(value) = maybe_value {
        let urls = parse_into_href_array(&value)
            .map_err(DeserializerError::custom)?;
        // Take first url
        urls.into_iter().next()
    } else {
        None
    };
    Ok(maybe_url)
}

#[derive(Deserialize)]
#[cfg_attr(test, derive(Default))]
#[serde(rename_all = "camelCase")]
pub struct Actor {
    pub id: String,

    #[serde(rename = "type")]
    pub object_type: String,

    pub name: Option<String>,
    pub preferred_username: String,

    pub inbox: String,
    pub outbox: String,
    pub followers: Option<String>,
    pub following: Option<String>,
    pub subscribers: Option<String>,
    pub featured: Option<String>,

    #[serde(default, deserialize_with = "deserialize_object_array")]
    pub assertion_method: Vec<Multikey>,
    #[serde(default)]
    pub authentication: Vec<Multikey>,

    pub public_key: PublicKey,

    #[serde(default, deserialize_with = "deserialize_image_opt")]
    pub icon: Option<ActorImage>,

    #[serde(default, deserialize_with = "deserialize_image_opt")]
    pub image: Option<ActorImage>,

    pub summary: Option<String>,

    pub also_known_as: Option<JsonValue>,

    #[serde(default, deserialize_with = "deserialize_object_array")]
    pub attachment: Vec<JsonValue>,

    #[serde(default)]
    pub manually_approves_followers: bool,

    #[serde(default, deserialize_with = "deserialize_object_array")]
    pub tag: Vec<JsonValue>,

    #[serde(default, deserialize_with = "deserialize_url_opt")]
    pub url: Option<String>,
}

impl Actor {
    pub fn address(
        &self,
    ) -> Result<ActorAddress, ValidationError> {
        let hostname = get_hostname(&self.id)
            .map_err(|_| ValidationError("invalid actor ID"))?;
        // Hostname is already normalized
        let actor_address = ActorAddress::new_unchecked(
            &self.preferred_username,
            &hostname,
        );
        Ok(actor_address)
    }

    pub fn into_db_actor(self) -> DbActor {
        DbActor {
            object_type: self.object_type,
            id: self.id,
            inbox: self.inbox,
            outbox: self.outbox,
            followers: self.followers,
            subscribers: self.subscribers,
            featured: self.featured,
            url: self.url,
            public_key: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INSTANCE_HOSTNAME: &str = "example.com";

    #[test]
    fn test_get_actor_address() {
        let actor = Actor {
            id: "https://test.org/users/1".to_string(),
            preferred_username: "test".to_string(),
            ..Default::default()
        };
        let actor_address = actor.address().unwrap();
        assert_eq!(actor_address.acct(INSTANCE_HOSTNAME), "test@test.org");
    }
}
