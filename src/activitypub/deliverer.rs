use std::collections::HashMap;

use futures::{
    stream::FuturesUnordered,
    StreamExt,
};
use reqwest::{Client, Method};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use mitra_activitypub::{
    http_client::{
        build_http_client,
        get_network_type,
        limited_response,
        RESPONSE_SIZE_LIMIT,
    },
};
use mitra_config::Instance;
use mitra_models::{
    profiles::types::PublicKeyType,
    users::types::User,
};
use mitra_utils::{
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
    urls::{get_hostname, UrlError},
};

use super::{
    agent::{build_federation_agent, FederationAgent},
    constants::AP_MEDIA_TYPE,
    identifiers::{local_actor_id, local_actor_key_id},
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
    UrlError(#[from] UrlError),

    #[error(transparent)]
    RequestError(#[from] reqwest::Error),

    #[error("http error {0:?}")]
    HttpError(reqwest::StatusCode),

    #[error("response size exceeds limit")]
    ResponseTooLarge,
}

fn build_deliverer_client(
    agent: &FederationAgent,
    request_url: &str,
) -> Result<Client, DelivererError> {
    let network = get_network_type(request_url)?;
    let http_client = build_http_client(
        agent,
        network,
        agent.deliverer_timeout,
    )?;
    Ok(http_client)
}

async fn send_activity(
    agent: &FederationAgent,
    activity_json: &str,
    inbox_url: &str,
) -> Result<(), DelivererError> {
    let headers = create_http_signature(
        Method::POST,
        inbox_url,
        activity_json,
        &agent.signer_key,
        &agent.signer_key_id,
    )?;

    let http_client = build_deliverer_client(agent, inbox_url)?;
    let request = http_client.post(inbox_url)
        .header("Host", headers.host)
        .header("Date", headers.date)
        .header("Digest", headers.digest.unwrap())
        .header("Signature", headers.signature)
        .header(reqwest::header::CONTENT_TYPE, AP_MEDIA_TYPE)
        .header(reqwest::header::USER_AGENT, &agent.user_agent)
        .body(activity_json.to_owned());

    if agent.is_instance_private {
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
            .chars()
            .filter(|chr| *chr != '\n' && *chr != '\r')
            .take(agent.deliverer_log_response_length)
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
    sender: User,
    activity: Value,
    recipients: &mut [Recipient],
) -> Result<(), DelivererError> {
    let actor_key = &sender.rsa_private_key;
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
                )?
            },
            _ => {
                sign_object_rsa(actor_key, &actor_key_id, &activity, None)?
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

    let agent = build_federation_agent(&instance, Some(&sender));
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
