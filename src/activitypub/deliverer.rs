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

use mitra_config::Instance;
use mitra_federation::{
    deliver::{send_activity, DelivererError},
};
use mitra_models::{
    profiles::types::PublicKeyType,
    users::types::User,
};
use mitra_utils::{
    crypto_eddsa::{
        ed25519_private_key_from_bytes,
        Ed25519PrivateKey,
    },
    crypto_rsa::{
        rsa_private_key_from_pkcs1_der,
        rsa_private_key_to_pkcs1_der,
        RsaPrivateKey,
    },
    json_signatures::create::{
        is_object_signed,
        sign_object_eddsa,
        sign_object_rsa,
    },
    urls::get_hostname,
};

use super::{
    agent::build_federation_agent_with_key,
    identifiers::{local_actor_id, local_actor_key_id},
};

fn deserialize_rsa_private_key<'de, D>(
    deserializer: D,
) -> Result<RsaPrivateKey, D::Error>
    where D: Deserializer<'de>
{
    let private_key_der = Vec::deserialize(deserializer)?;
    let private_key = rsa_private_key_from_pkcs1_der(&private_key_der)
        .map_err(DeserializerError::custom)?;
    Ok(private_key)
}

fn serialize_rsa_private_key<S>(
    private_key: &RsaPrivateKey,
    serializer: S,
) -> Result<S::Ok, S::Error>
    where S: Serializer,
{
    let private_key_der = rsa_private_key_to_pkcs1_der(private_key)
        .map_err(S::Error::custom)?;
    Vec::serialize(&private_key_der, serializer)
}

fn deserialize_ed25519_private_key<'de, D>(
    deserializer: D,
) -> Result<Option<Ed25519PrivateKey>, D::Error>
    where D: Deserializer<'de>
{
    let maybe_private_key_bytes: Option<Vec<u8>> =
        Option::deserialize(deserializer)?;
    let maybe_private_key = if let Some(private_key_bytes) = maybe_private_key_bytes {
        let private_key = ed25519_private_key_from_bytes(&private_key_bytes)
            .map_err(DeserializerError::custom)?;
        Some(private_key)
    } else {
        None
    };
    Ok(maybe_private_key)
}

fn serialize_ed25519_private_key<S>(
    maybe_private_key: &Option<Ed25519PrivateKey>,
    serializer: S,
) -> Result<S::Ok, S::Error>
    where S: Serializer,
{
    Option::serialize(maybe_private_key, serializer)
}

#[derive(Clone, Deserialize, Serialize)]
pub struct Sender {
    pub(super) username: String,
    #[serde(
        deserialize_with = "deserialize_rsa_private_key",
        serialize_with = "serialize_rsa_private_key",
    )]
    pub(super) rsa_private_key: RsaPrivateKey,
    #[serde(
        deserialize_with = "deserialize_ed25519_private_key",
        serialize_with = "serialize_ed25519_private_key",
    )]
    pub(super) ed25519_private_key: Option<Ed25519PrivateKey>,
}

impl From<User> for Sender {
    fn from(user: User) -> Self {
        Self {
            username: user.profile.username,
            rsa_private_key: user.rsa_private_key,
            ed25519_private_key: user.ed25519_private_key,
        }
    }
}

#[derive(Deserialize, Serialize)]
pub struct Recipient {
    pub id: String,
    pub(super) inbox: String,

    // Default to false if serialized data contains no value.
    #[serde(default)]
    pub is_delivered: bool,

    // This flag is set after first failed delivery attempt
    // if the recipient had prior unreachable status.
    // Default to false if serialized data contains no value.
    #[serde(default)]
    pub is_unreachable: bool,
}

impl Recipient {
    pub fn is_finished(&self) -> bool {
        self.is_delivered || self.is_unreachable
    }
}

const DELIVERY_BATCH_SIZE: usize = 5;

pub(super) async fn deliver_activity_worker(
    instance: Instance,
    sender: Sender,
    activity: JsonValue,
    recipients: &mut [Recipient],
) -> Result<(), DelivererError> {
    let rsa_private_key = sender.rsa_private_key;
    let actor_id = local_actor_id(
        &instance.url(),
        &sender.username,
    );
    let rsa_key_id = local_actor_key_id(&actor_id, PublicKeyType::RsaPkcs1);

    let activity_signed = if is_object_signed(&activity) {
        log::warn!("activity is already signed");
        activity
    } else {
        match sender.ed25519_private_key {
            Some(ref ed25519_private_key) if instance.fep_8b32_eddsa_enabled => {
                let ed25519_key_id = local_actor_key_id(
                    &actor_id,
                    PublicKeyType::Ed25519,
                );
                sign_object_eddsa(
                    ed25519_private_key,
                    &ed25519_key_id,
                    &activity,
                    None,
                    false, // use eddsa-jcs-2022
                )?
            },
            _ => {
                sign_object_rsa(
                    &rsa_private_key,
                    &rsa_key_id,
                    &activity,
                    None,
                )?
            },
        }
    };
    let activity_json = serde_json::to_string(&activity_signed)?;

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
        rsa_private_key,
        rsa_key_id,
    );
    let mut delivery_pool = FuturesUnordered::new();
    let mut delivery_pool_state = HashMap::new();

    loop {
        for (index, hostname, ref inbox) in deliveries.iter() {
            // Add deliveries to the pool until it is full
            if delivery_pool_state.len() == DELIVERY_BATCH_SIZE {
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
                let result = send_activity(
                    &agent,
                    &activity_json,
                    inbox,
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
            if let Err(error) = result {
                log::warn!(
                    "failed to deliver activity to {}: {}",
                    recipient.inbox,
                    error,
                );
            } else {
                recipient.is_delivered = true;
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
    use mitra_utils::{
        crypto_eddsa::generate_weak_ed25519_key,
        crypto_rsa::generate_weak_rsa_key,
    };
    use super::*;

    #[test]
    fn test_sender_serialization_deserialization() {
        let rsa_private_key = generate_weak_rsa_key().unwrap();
        let ed25519_private_key = generate_weak_ed25519_key();
        let sender = Sender {
            username: "test".to_string(),
            rsa_private_key: rsa_private_key.clone(),
            ed25519_private_key: Some(ed25519_private_key),
        };
        let value = serde_json::to_value(sender).unwrap();
        let sender: Sender = serde_json::from_value(value).unwrap();
        assert_eq!(sender.rsa_private_key, rsa_private_key);
        assert_eq!(sender.ed25519_private_key, Some(ed25519_private_key));
    }
}
