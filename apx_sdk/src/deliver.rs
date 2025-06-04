//! Deliver activities
use reqwest::{header, Client, StatusCode};
use thiserror::Error;

use apx_core::{
    http_signatures::create::{
        create_http_signature_cavage,
        HttpSignatureError,
    },
    http_types::Method,
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

#[derive(Debug)]
pub struct Response {
    pub status: StatusCode,
    pub body: String,
}

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

    #[error("response size exceeds limit")]
    ResponseTooLarge,

    #[error("HTTP error {}", .0.status.as_u16())]
    HttpError(Response),
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
) -> Result<Response, DelivererError> {
    if agent.ssrf_protection_enabled {
        require_safe_url(inbox_url)?;
    };

    let http_client = build_deliverer_client(agent, inbox_url)?;
    let mut request_builder = http_client.post(inbox_url)
        .header(header::CONTENT_TYPE, AP_MEDIA_TYPE);
    if let Some(ref user_agent) = agent.user_agent {
        request_builder = request_builder
            .header(header::USER_AGENT, user_agent);
    };
    if let Some(ref signer) = agent.signer {
         let headers = create_http_signature_cavage(
            Method::POST,
            inbox_url,
            object_json.as_bytes(),
            signer,
        )?;
        let digest = headers.digest
            .expect("digest header should be present if method is POST");
        request_builder = request_builder
            .header(header::HOST, headers.host)
            .header(header::DATE, headers.date)
            .header("Digest", digest)
            .header("Signature", headers.signature);
    };
    for (name, value) in extra_headers {
        request_builder = request_builder.header(*name, *value);
    };

    let response = request_builder
        .body(object_json.to_owned())
        .send()
        .await?;
    let response_status = response.status();
    let response_data = limited_response(response, agent.response_size_limit)
        .await
        .ok_or(DelivererError::ResponseTooLarge)?;
    let response_text = String::from_utf8(response_data.to_vec())
        // Replace non-UTF8 responses with empty string
        .unwrap_or_default();
    let response = Response { status: response_status, body: response_text };
    // https://www.w3.org/wiki/ActivityPub/Primer/HTTP_status_codes_for_delivery
    if response_status.is_success() {
        Ok(response)
    } else {
        Err(DelivererError::HttpError(response))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_error_to_string() {
        let response = Response {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            body: "".to_string(),
        };
        let error = DelivererError::HttpError(response);
        assert_eq!(error.to_string(), "HTTP error 500");
    }
}
