//! Delivering activities.

use bytes::Bytes;
use reqwest::{header, Client, Method, StatusCode};
use serde_json::{Value as JsonValue};
use thiserror::Error;

use apx_core::{
    http_signatures::create::HttpSignatureError,
    http_url_whatwg::UrlError,
};

use crate::{
    agent::FederationAgent,
    constants::AP_MEDIA_TYPE,
    http_client::{
        build_http_request,
        create_http_client,
        describe_request_error,
        get_network_type,
        limited_response,
        sign_http_request,
        RedirectAction,
        UnsafeUrlError,
    },
};

#[derive(Debug)]
pub struct Response {
    pub status: StatusCode,
    pub body: String,
}

impl Response {
    fn new(status: StatusCode, body: Bytes) -> Self {
        let body_text = String::from_utf8(body.to_vec())
            // Replace non-UTF8 responses with empty string
            .unwrap_or_default();
        Self { status: status, body: body_text }
    }
}

#[derive(Debug, Error)]
pub enum DelivererError {
    #[error(transparent)]
    HttpSignatureError(#[from] HttpSignatureError),

    #[error("object serialization error")]
    SerializationError(#[from] serde_json::Error),

    #[error("inavlid URL")]
    UrlError(#[from] UrlError),

    #[error(transparent)]
    UnsafeUrl(#[from] UnsafeUrlError),

    #[error("{}", describe_request_error(.0))]
    RequestError(#[from] reqwest::Error),

    #[error("response size exceeds limit")]
    ResponseTooLarge,

    #[error("HTTP error {}", .0.status.as_u16())]
    HttpError(Response),
}

fn create_deliverer_client(
    agent: &FederationAgent,
    request_url: &str,
) -> Result<Client, DelivererError> {
    let network = get_network_type(request_url)?;
    let client = create_http_client(
        agent,
        network,
        agent.deliverer_timeout,
        RedirectAction::None, // do not follow redirects
    )?;
    Ok(client)
}

/// Delivers object to inbox or outbox
pub async fn send_object(
    agent: &FederationAgent,
    inbox_url: &str,
    object_json: &JsonValue,
    extra_headers: &[(&str, &str)],
) -> Result<Response, DelivererError> {
    let client = create_deliverer_client(agent, inbox_url)?;
    let request_body = serde_json::to_string(object_json)?;
    let mut request_builder = build_http_request(
        agent,
        &client,
        Method::POST,
        inbox_url,
    )?;
    request_builder = request_builder
        .header(header::CONTENT_TYPE, AP_MEDIA_TYPE);
    if let Some(ref signer) = agent.signer {
        request_builder = sign_http_request(
            request_builder,
            Method::POST,
            inbox_url,
            request_body.as_bytes(),
            signer,
        )?;
    };
    for (name, value) in extra_headers {
        request_builder = request_builder.header(*name, *value);
    };

    let response = request_builder
        .body(request_body)
        .send()
        .await?;
    let response_status = response.status();
    let response_data = limited_response(response, agent.response_size_limit)
        .await
        .ok_or(DelivererError::ResponseTooLarge)?;
    let response = Response::new(response_status, response_data);
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
