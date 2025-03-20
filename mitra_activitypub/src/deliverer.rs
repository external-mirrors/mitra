use std::collections::HashMap;

use futures::{
    stream::FuturesUnordered,
    StreamExt,
};
use serde::{
    Deserialize,
    Deserializer,
    Serialize,
    Serializer,
    de::{Error as DeserializerError},
    ser::{Error as _},
};
use serde_json::{Value as JsonValue};

use apx_core::{
    crypto_eddsa::{
        ed25519_public_key_from_secret_key,
        ed25519_secret_key_from_bytes,
        Ed25519SecretKey,
    },
    crypto_rsa::{
        rsa_public_key_to_pkcs1_der,
        rsa_secret_key_from_pkcs1_der,
        rsa_secret_key_to_pkcs1_der,
        RsaPublicKey,
        RsaSecretKey,
    },
    json_signatures::create::{
        is_object_signed,
        sign_object,
        JsonSignatureError,
    },
    urls::get_hostname,
};
use apx_sdk::{
    deliver::{send_object, DelivererError},
};
use mitra_config::Instance;
use mitra_models::{
    profiles::types::PublicKeyType,
    users::types::{PortableUser, User},
};

use crate::{
    agent::build_federation_agent_with_key,
    identifiers::{local_actor_id, local_actor_key_id},
    utils::db_url_to_http_url,
};

fn deserialize_rsa_secret_key<'de, D>(
    deserializer: D,
) -> Result<RsaSecretKey, D::Error>
    where D: Deserializer<'de>
{
    let secret_key_der = Vec::deserialize(deserializer)?;
    let secret_key = rsa_secret_key_from_pkcs1_der(&secret_key_der)
        .map_err(DeserializerError::custom)?;
    Ok(secret_key)
}

fn serialize_rsa_secret_key<S>(
    secret_key: &RsaSecretKey,
    serializer: S,
) -> Result<S::Ok, S::Error>
    where S: Serializer,
{
    let secret_key_der = rsa_secret_key_to_pkcs1_der(secret_key)
        .map_err(S::Error::custom)?;
    Vec::serialize(&secret_key_der, serializer)
}

fn deserialize_ed25519_secret_key<'de, D>(
    deserializer: D,
) -> Result<Option<Ed25519SecretKey>, D::Error>
    where D: Deserializer<'de>
{
    let maybe_secret_key_bytes: Option<Vec<u8>> =
        Option::deserialize(deserializer)?;
    let maybe_secret_key = if let Some(secret_key_bytes) = maybe_secret_key_bytes {
        let secret_key = ed25519_secret_key_from_bytes(&secret_key_bytes)
            .map_err(DeserializerError::custom)?;
        Some(secret_key)
    } else {
        None
    };
    Ok(maybe_secret_key)
}

fn serialize_ed25519_secret_key<S>(
    maybe_secret_key: &Option<Ed25519SecretKey>,
    serializer: S,
) -> Result<S::Ok, S::Error>
    where S: Serializer,
{
    Option::serialize(maybe_secret_key, serializer)
}

#[derive(Clone, Deserialize, Serialize)]
pub struct Sender {
    username: String,

    #[serde(
        alias = "rsa_private_key",
        deserialize_with = "deserialize_rsa_secret_key",
        serialize_with = "serialize_rsa_secret_key",
    )]
    rsa_secret_key: RsaSecretKey,
    rsa_key_id: Option<String>,

    #[serde(
        alias = "ed25519_private_key",
        deserialize_with = "deserialize_ed25519_secret_key",
        serialize_with = "serialize_ed25519_secret_key",
    )]
    ed25519_secret_key: Option<Ed25519SecretKey>,
    ed25519_key_id: Option<String>,
}

impl Sender {
    pub fn from_user(instance_url: &str, user: &User) -> Self {
        let actor_id = local_actor_id(
            instance_url,
            &user.profile.username,
        );
        let rsa_key_id = local_actor_key_id(
            &actor_id,
            PublicKeyType::RsaPkcs1,
        );
        let ed25519_key_id = local_actor_key_id(
            &actor_id,
            PublicKeyType::Ed25519,
        );
        Self {
            username: user.profile.username.clone(),
            rsa_secret_key: user.rsa_secret_key.clone(),
            rsa_key_id: Some(rsa_key_id),
            ed25519_secret_key: Some(user.ed25519_secret_key),
            ed25519_key_id: Some(ed25519_key_id),
        }
    }

    // Returns None if the registered secret key doesn't correspond to
    // any of public keys associated with the actor
    pub fn from_portable_user(
        instance_url: &str,
        user: &PortableUser,
    ) -> Option<Self> {
        let rsa_public_key = RsaPublicKey::from(&user.rsa_secret_key);
        let rsa_public_key_der = rsa_public_key_to_pkcs1_der(&rsa_public_key)
            .expect("RSA key should be serializable");
        let rsa_key_id = &user.profile.public_keys
            .find_by_value(&rsa_public_key_der)?
            .id;
        let http_rsa_key_id = db_url_to_http_url(rsa_key_id, instance_url)
            .expect("RSA key ID should be valid");
        let ed25519_public_key =
            ed25519_public_key_from_secret_key(&user.ed25519_secret_key);
        let ed25519_key_id = &user.profile.public_keys
            .find_by_value(ed25519_public_key.as_bytes())?
            .id;
        let http_ed25519_key_id = db_url_to_http_url(ed25519_key_id, instance_url)
            .expect("RSA key ID should be valid");
        let sender = Self {
            username: user.profile.username.clone(),
            rsa_secret_key: user.rsa_secret_key.clone(),
            rsa_key_id: Some(http_rsa_key_id),
            ed25519_secret_key: Some(user.ed25519_secret_key),
            ed25519_key_id: Some(http_ed25519_key_id),
        };
        Some(sender)
    }
}

/// Represents delivery to a single inbox
#[derive(Deserialize, Serialize)]
pub struct Recipient {
    pub id: String,
    pub(super) inbox: String,

    pub is_delivered: bool,

    // This flag is set after first failed delivery attempt
    // if the recipient had prior unreachable status.
    pub is_unreachable: bool,

    // Local portable actor (HTTP request is not needed)
    #[serde(default)]
    pub is_local: bool,
}

impl Recipient {
    pub fn is_finished(&self) -> bool {
        self.is_delivered || self.is_unreachable
    }
}

pub(super) fn sign_activity(
    instance_url: &str,
    sender: &User,
    activity: JsonValue,
) -> Result<JsonValue, JsonSignatureError> {
    let actor_id = local_actor_id(
        instance_url,
        &sender.profile.username,
    );
    let activity_signed = if is_object_signed(&activity) {
        log::warn!("activity is already signed");
        activity
    } else {
        let ed25519_key_id = local_actor_key_id(
            &actor_id,
            PublicKeyType::Ed25519,
        );
        sign_object(
            &sender.ed25519_secret_key,
            &ed25519_key_id,
            &activity,
        )?
    };
    Ok(activity_signed)
}

fn truncate_response(body: &str, limit: usize) -> String {
    body.chars()
        .filter(|chr| *chr != '\n' && *chr != '\r')
        .take(limit)
        .collect()
}

pub(super) async fn deliver_activity_worker(
    instance: Instance,
    sender: Sender,
    activity: JsonValue,
    recipients: &mut [Recipient],
) -> Result<(), DelivererError> {
    assert!(instance.federation.enabled);
    let rsa_secret_key = sender.rsa_secret_key;
    let rsa_key_id = if let Some(rsa_key_id) = sender.rsa_key_id {
        rsa_key_id
    } else {
        log::warn!("deliverer job data doesn't contain key ID");
        let actor_id = local_actor_id(
            &instance.url(),
            &sender.username,
        );
        local_actor_key_id(&actor_id, PublicKeyType::RsaPkcs1)
    };
    let activity_json = serde_json::to_string(&activity)?;

    let mut deliveries = vec![];
    let mut sent = vec![];

    for (index, recipient) in recipients.iter().enumerate() {
        if recipient.is_finished() {
            continue;
        };
        let hostname = get_hostname(&recipient.inbox)?;
        deliveries.push((index, hostname, recipient.inbox.clone()));
    };

    let agent = build_federation_agent_with_key(
        &instance,
        rsa_secret_key,
        rsa_key_id,
    );
    let mut delivery_pool = FuturesUnordered::new();
    let mut delivery_pool_state = HashMap::new();

    loop {
        for (index, hostname, ref inbox) in deliveries.iter() {
            // Add deliveries to the pool until it is full
            if delivery_pool_state.len() == instance.federation.deliverer_pool_size {
                break;
            };
            if sent.contains(index) {
                // Already queued
                continue;
            };
            if delivery_pool_state.values()
                .any(|current_hostname| current_hostname == &hostname)
            {
                // Another delivery to instance is in progress
                continue;
            };
            // Deliver activities concurrently
            let future = async {
                let result = send_object(
                    &agent,
                    &activity_json,
                    inbox,
                    &[],
                ).await;
                (*index, result)
            };
            delivery_pool.push(future);
            delivery_pool_state.insert(index, hostname);
            sent.push(*index);
        };
        // Await one delivery at a time
        if let Some((index, result)) = delivery_pool.next().await {
            delivery_pool_state.remove(&index)
                .expect("delivery should be tracked by pool state");
            let recipient = recipients.get_mut(index)
                .expect("index should not be out of bounds");
            match result {
                Ok(Some(response)) => {
                    assert!(response.status.is_success());
                    let response_text = truncate_response(
                        &response.body,
                        instance.federation.deliverer_log_response_length,
                    );
                    log::info!(
                        "response from {}: [{}] {}",
                        recipient.inbox,
                        response.status.as_str(),
                        response_text,
                    );
                    recipient.is_delivered = true;
                },
                Ok(None) => {
                    recipient.is_delivered = true;
                },
                Err(error) => {
                    let error_message = match error {
                        DelivererError::HttpError(ref response) => {
                            let response_text = truncate_response(
                                &response.body,
                                instance.federation.deliverer_log_response_length,
                            );
                            format!(
                                "{}: [{}] {}",
                                error,
                                response.status.as_str(),
                                response_text,
                            )
                        },
                        _ => error.to_string(),
                    };
                    log::warn!(
                        "failed to deliver activity to {}: {}",
                        recipient.inbox,
                        error_message,
                    );
                },
            };
        };
        if delivery_pool_state.is_empty() &&
            deliveries.iter().all(|(index, ..)| sent.contains(index))
        {
            // No deliveries left, exit
            let closed_pool: Vec<_> = delivery_pool.collect().await;
            assert!(closed_pool.is_empty());
            break;
        };
    };
    Ok(())
}

#[cfg(test)]
mod tests {
    use apx_core::{
        crypto_eddsa::generate_weak_ed25519_key,
        crypto_rsa::generate_weak_rsa_key,
    };
    use super::*;

    #[test]
    fn test_sender_serialization_deserialization() {
        let rsa_secret_key = generate_weak_rsa_key().unwrap();
        let ed25519_secret_key = generate_weak_ed25519_key();
        let sender = Sender {
            username: "test".to_string(),
            rsa_secret_key: rsa_secret_key.clone(),
            rsa_key_id: Some("https://social.example/rsa-key".to_string()),
            ed25519_secret_key: Some(ed25519_secret_key),
            ed25519_key_id: Some("https://social.example/ed25519-key".to_string()),
        };
        let value = serde_json::to_value(sender).unwrap();
        let sender: Sender = serde_json::from_value(value).unwrap();
        assert_eq!(sender.rsa_secret_key, rsa_secret_key);
        assert_eq!(sender.ed25519_secret_key, Some(ed25519_secret_key));
    }

    #[test]
    fn test_sender_serialization_deserialization_legacy() {
        let rsa_secret_key = generate_weak_rsa_key().unwrap();
        let ed25519_secret_key = generate_weak_ed25519_key();
        let sender = Sender {
            username: "test".to_string(),
            rsa_secret_key: rsa_secret_key.clone(),
            rsa_key_id: Some("https://social.example/rsa-key".to_string()),
            ed25519_secret_key: Some(ed25519_secret_key),
            ed25519_key_id: Some("https://social.example/ed25519-key".to_string()),
        };
        let value = serde_json::to_value(sender).unwrap();
        let rsa_secret_key_json = &value["rsa_secret_key"];
        let ed25519_secret_key_json = &value["ed25519_secret_key"];
        let value = serde_json::json!({
            "username": "test",
            "rsa_private_key": rsa_secret_key_json,
            "ed25519_private_key": ed25519_secret_key_json,
        });
        let sender: Sender = serde_json::from_value(value).unwrap();
        assert_eq!(sender.rsa_secret_key, rsa_secret_key);
        assert_eq!(sender.rsa_key_id, None);
        assert_eq!(sender.ed25519_secret_key, Some(ed25519_secret_key));
        assert_eq!(sender.ed25519_key_id, None);
    }
}
