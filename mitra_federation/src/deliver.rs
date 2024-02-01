use reqwest::{Client, Method};
use thiserror::Error;

use mitra_utils::{
    http_signatures::create::{
        create_http_signature,
        HttpSignatureError,
    },
    json_signatures::create::JsonSignatureError,
    urls::UrlError,
};

use crate::{
    agent::FederationAgent,
    constants::AP_MEDIA_TYPE,
    http_client::{
        build_http_client,
        get_network_type,
        limited_response,
        RESPONSE_SIZE_LIMIT,
    },
};

#[derive(Debug, Error)]
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

pub async fn send_activity(
    agent: &FederationAgent,
    activity_json: &str,
    inbox_url: &str,
) -> Result<(), DelivererError> {
    let headers = create_http_signature(
        Method::POST,
        inbox_url,
        activity_json.as_bytes(),
        &agent.signer_key,
        &agent.signer_key_id,
    )?;

    let http_client = build_deliverer_client(agent, inbox_url)?;
    let digest = headers.digest
        .expect("digest header should be present if method is POST");
    let request = http_client.post(inbox_url)
        .header("Host", headers.host)
        .header("Date", headers.date)
        .header("Digest", digest)
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
        let mut response = request.send().await?;
        let response_status = response.status();
        let response_data = limited_response(&mut response, RESPONSE_SIZE_LIMIT)
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
