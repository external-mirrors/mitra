use std::collections::{BTreeMap, HashMap};

use futures::{
    stream::FuturesUnordered,
    StreamExt,
};
use reqwest::{Client, Method};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use mitra_config::Instance;
use mitra_models::{
    database::{
        DatabaseClient,
        DatabaseError,
    },
    profiles::types::{DbActor, PublicKeyType},
    users::types::User,
};
use mitra_utils::{
    crypto_rsa::RsaPrivateKey,
    http_signatures::create::{
        create_http_signature,
        HttpSignatureError,
    },
    json_signatures::create::{
        is_object_signed,
        sign_object_eddsa,
        sign_object_rsa,
        JsonSignatureError,
    },
    urls::get_hostname,
};

use super::{
    constants::AP_MEDIA_TYPE,
    http_client::{
        build_federation_client,
        get_network_type,
        limited_response,
        RESPONSE_SIZE_LIMIT,
    },
    identifiers::{local_actor_id, local_actor_key_id},
    queues::OutgoingActivityJobData,
};

#[derive(thiserror::Error, Debug)]
pub enum DelivererError {
    #[error(transparent)]
    HttpSignatureError(#[from] HttpSignatureError),

    #[error(transparent)]
    JsonSignatureError(#[from] JsonSignatureError),

    #[error("activity serialization error")]
    SerializationError(#[from] serde_json::Error),

    #[error("inavlid URL")]
    UrlError(#[from] url::ParseError),

    #[error(transparent)]
    RequestError(#[from] reqwest::Error),

    #[error("http error {0:?}")]
    HttpError(reqwest::StatusCode),

    #[error("response size exceeds limit")]
    ResponseTooLarge,
}

fn build_client(
    instance: &Instance,
    request_url: &str,
) -> Result<Client, DelivererError> {
    let network = get_network_type(request_url)?;
    let client = build_federation_client(
        instance,
        network,
        instance.deliverer_timeout,
    )?;
    Ok(client)
}

async fn send_activity(
    instance: &Instance,
    actor_key: &RsaPrivateKey,
    actor_key_id: &str,
    activity_json: &str,
    inbox_url: &str,
) -> Result<(), DelivererError> {
    let headers = create_http_signature(
        Method::POST,
        inbox_url,
        activity_json,
        actor_key,
        actor_key_id,
    )?;

    let client = build_client(instance, inbox_url)?;
    let request = client.post(inbox_url)
        .header("Host", headers.host)
        .header("Date", headers.date)
        .header("Digest", headers.digest.unwrap())
        .header("Signature", headers.signature)
        .header(reqwest::header::CONTENT_TYPE, AP_MEDIA_TYPE)
        .header(reqwest::header::USER_AGENT, instance.agent())
        .body(activity_json.to_owned());

    if instance.is_private {
        log::info!(
            "private mode: not sending activity to {}",
            inbox_url,
        );
    } else {
        let response = request.send().await?;
        let response_status = response.status();
        let response_data = limited_response(response, RESPONSE_SIZE_LIMIT)
            .await?
            .ok_or(DelivererError::ResponseTooLarge)?;
        let response_text: String = String::from_utf8(response_data.to_vec())
            // Replace non-UTF8 responses with empty string
            .unwrap_or_default()
            .chars().filter(|chr| *chr != '\n' && *chr != '\r').take(75)
            .collect();
        log::info!(
            "response from {}: [{}] {}",
            inbox_url,
            response_status.as_str(),
            response_text,
        );
        if response_status.is_client_error() || response_status.is_server_error() {
            return Err(DelivererError::HttpError(response_status));
        };
    };
    Ok(())
}

#[derive(Deserialize, Serialize)]
pub struct Recipient {
    pub id: String,
    inbox: String,

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

async fn deliver_activity_worker(
    instance: Instance,
    sender: User,
    activity: Value,
    recipients: &mut [Recipient],
) -> Result<(), DelivererError> {
    let actor_key = sender.rsa_private_key;
    let actor_id = local_actor_id(
        &instance.url(),
        &sender.profile.username,
    );
    let actor_key_id = local_actor_key_id(&actor_id, PublicKeyType::RsaPkcs1);

    let activity_signed = if is_object_signed(&activity) {
        log::warn!("activity is already signed");
        activity
    } else {
        match sender.ed25519_private_key {
            Some(ed25519_private_key) if instance.fep_8b32_eddsa_enabled => {
                let ed25519_key_id = local_actor_key_id(
                    &actor_id,
                    PublicKeyType::Ed25519,
                );
                sign_object_eddsa(
                    ed25519_private_key.inner(),
                    &ed25519_key_id,
                    &activity,
                    None,
                )?
            },
            _ => {
                sign_object_rsa(&actor_key, &actor_key_id, &activity, None)?
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
                    &instance,
                    &actor_key,
                    &actor_key_id,
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

pub struct OutgoingActivity {
    pub instance: Instance,
    pub sender: User,
    pub activity: Value,
    pub recipients: Vec<Recipient>,
}

impl OutgoingActivity {
    pub fn new(
        instance: &Instance,
        sender: &User,
        activity: impl Serialize,
        recipients: Vec<DbActor>,
    ) -> Self {
        // Sort and de-duplicate recipients
        let mut recipient_map = BTreeMap::new();
        for actor in recipients {
            if !recipient_map.contains_key(&actor.id) {
                let recipient = Recipient {
                    id: actor.id.clone(),
                    inbox: actor.inbox,
                    is_delivered: false,
                    is_unreachable: false,
                };
                recipient_map.insert(actor.id, recipient);
            };
        };
        Self {
            instance: instance.clone(),
            sender: sender.clone(),
            activity: serde_json::to_value(activity)
                .expect("activity should be serializable"),
            recipients: recipient_map.into_values().collect(),
        }
    }

    pub(super) async fn deliver(
        mut self,
    ) -> Result<Vec<Recipient>, DelivererError> {
        deliver_activity_worker(
            self.instance,
            self.sender,
            self.activity,
            &mut self.recipients,
        ).await?;
        Ok(self.recipients)
    }

    pub async fn enqueue(
        self,
        db_client: &impl DatabaseClient,
    ) -> Result<(), DatabaseError> {
        if self.recipients.is_empty() {
            return Ok(());
        };
        log::info!(
            "delivering activity to {} inboxes: {}",
            self.recipients.len(),
            self.activity,
        );
        let job_data = OutgoingActivityJobData {
            activity: self.activity,
            sender_id: self.sender.id,
            recipients: self.recipients,
            failure_count: 0,
        };
        job_data.into_job(db_client, 0).await
    }
}
