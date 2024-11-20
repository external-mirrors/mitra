use reqwest::{Client, Method};
use thiserror::Error;

use apx_core::{
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
        require_safe_url,
        UnsafeUrlError,
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
    UnsafeUrl(#[from] UnsafeUrlError),

    #[error(transparent)]
    RequestError(#[from] reqwest::Error),

    #[error("http error: [{status:?}] {text}")]
    HttpError { status: reqwest::StatusCode, text: String },

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
        true, // do not follow redirects
    )?;
    Ok(http_client)
}

/// Delivers object to inbox or outbox
pub async fn send_object(
    agent: &FederationAgent,
    object_json: &str,
    inbox_url: &str,
    extra_headers: &[(&str, &str)],
) -> Result<(), DelivererError> {
    if agent.ssrf_protection_enabled {
        require_safe_url(inbox_url)?;
    };
    let headers = create_http_signature(
        Method::POST,
        inbox_url,
        object_json.as_bytes(),
        &agent.signer_key,
        &agent.signer_key_id,
    )?;

    let http_client = build_deliverer_client(agent, inbox_url)?;
    let digest = headers.digest
        .expect("digest header should be present if method is POST");
    let mut request_builder = http_client.post(inbox_url)
        .header("Host", headers.host)
        .header("Date", headers.date)
        .header("Digest", digest)
        .header("Signature", headers.signature)
        .header(reqwest::header::CONTENT_TYPE, AP_MEDIA_TYPE)
        .header(reqwest::header::USER_AGENT, &agent.user_agent);
    for (name, value) in extra_headers {
        request_builder = request_builder.header(*name, *value);
    };

    if agent.is_instance_private {
        log::info!(
            "private mode: not delivering to {}",
            inbox_url,
        );
        return Ok(());
    };

    let mut response = request_builder
        .body(object_json.to_owned())
        .send()
        .await?;
    let response_status = response.status();
    let response_data = limited_response(&mut response, agent.response_size_limit)
        .await?
        .ok_or(DelivererError::ResponseTooLarge)?;
    let response_text: String = String::from_utf8(response_data.to_vec())
        // Replace non-UTF8 responses with empty string
        .unwrap_or_default()
        .chars()
        .filter(|chr| *chr != '\n' && *chr != '\r')
        .take(agent.deliverer_log_response_length)
        .collect();
    // https://www.w3.org/wiki/ActivityPub/Primer/HTTP_status_codes_for_delivery
    if response_status.is_success() {
        log::info!(
            "response from {}: [{}] {}",
            inbox_url,
            response_status.as_str(),
            response_text,
        );
    } else {
        return Err(DelivererError::HttpError {
            status: response_status,
            text: response_text,
        });
    };
    Ok(())
}
