use bytes::Bytes;
use reqwest::{Client, Method, RequestBuilder, StatusCode};
use serde::de::DeserializeOwned;

use mitra_utils::{
    files::sniff_media_type,
    http_signatures::create::{
        create_http_signature,
        HttpSignatureError,
    },
    urls::{is_safe_url, UrlError},
};

use super::{
    agent::FederationAgent,
    constants::AP_MEDIA_TYPE,
    http_client::{
        build_http_client,
        get_network_type,
        limited_response,
        RESPONSE_SIZE_LIMIT,
    },
};

#[derive(thiserror::Error, Debug)]
pub enum FetchError {
    #[error(transparent)]
    SignatureError(#[from] HttpSignatureError),

    #[error("inavlid URL")]
    UrlError(#[from] UrlError),

    #[error("invalid URL")]
    UnsafeUrl,

    #[error(transparent)]
    RequestError(#[from] reqwest::Error),

    #[error("resource not found: {0}")]
    NotFound(String),

    #[error("response size exceeds limit")]
    ResponseTooLarge,

    #[error("json parse error: {0}")]
    JsonParseError(#[from] serde_json::Error),

    #[error("unsupported media type: {0}")]
    UnsupportedMediaType(String),

    #[error("too many objects")]
    RecursionError,
}

fn build_fetcher_client(
    agent: &FederationAgent,
    request_url: &str,
) -> Result<Client, FetchError> {
    let network = get_network_type(request_url)?;
    let http_client = build_http_client(
        agent,
        network,
        agent.fetcher_timeout,
    )?;
    Ok(http_client)
}

fn build_request(
    agent: &FederationAgent,
    http_client: Client,
    method: Method,
    url: &str,
) -> RequestBuilder {
    let mut request_builder = http_client.request(method, url);
    if !agent.is_instance_private {
        // Public instances should set User-Agent header
        request_builder = request_builder
            .header(reqwest::header::USER_AGENT, &agent.user_agent);
    };
    request_builder
}

fn fetcher_error_for_status(error: reqwest::Error) -> FetchError {
    match (error.url(), error.status()) {
        (Some(url), Some(StatusCode::NOT_FOUND)) => {
            FetchError::NotFound(url.to_string())
        },
        _ => error.into(),
    }
}

/// Sends GET request to fetch AP object
async fn send_request(
    agent: &FederationAgent,
    url: &str,
) -> Result<Bytes, FetchError> {
    let http_client = build_fetcher_client(agent, url)?;
    let mut request_builder =
        build_request(agent, http_client, Method::GET, url)
            .header(reqwest::header::ACCEPT, AP_MEDIA_TYPE);

    if !agent.is_instance_private {
        // Only public instances can send signed requests
        let headers = create_http_signature(
            Method::GET,
            url,
            b"",
            &agent.signer_key,
            &agent.signer_key_id,
        )?;
        request_builder = request_builder
            .header("Host", headers.host)
            .header("Date", headers.date)
            .header("Signature", headers.signature);
    };

    let response = request_builder
        .send().await?
        .error_for_status()
        .map_err(fetcher_error_for_status)?;
    let data = limited_response(response, RESPONSE_SIZE_LIMIT)
        .await?
        .ok_or(FetchError::ResponseTooLarge)?;
    Ok(data)
}

pub async fn fetch_object<T: DeserializeOwned>(
    agent: &FederationAgent,
    object_url: &str,
) -> Result<T, FetchError> {
    let object_json = send_request(agent, object_url).await?;
    let object: T = serde_json::from_slice(&object_json)?;
    Ok(object)
}

fn get_media_type(
    file_data: &[u8],
    maybe_media_type: Option<&str>,
    default_media_type: Option<&str>,
) -> String {
    maybe_media_type
        .or(default_media_type)
        .map(|media_type| media_type.to_string())
        // Ignore if reported media type is application/octet-stream
        .filter(|media_type| media_type != "application/octet-stream")
        // Sniff media type if not provided
        .or(sniff_media_type(file_data))
        .unwrap_or("application/octet-stream".to_string())
}

pub async fn fetch_file(
    agent: &FederationAgent,
    url: &str,
    expected_media_type: Option<&str>,
    allowed_media_types: &[&str],
    file_max_size: usize,
) -> Result<(Vec<u8>, usize, String), FetchError> {
    if !is_safe_url(url) {
        return Err(FetchError::UnsafeUrl);
    };
    let http_client = build_fetcher_client(agent, url)?;
    let request_builder =
        build_request(agent, http_client, Method::GET, url);
    let response = request_builder.send().await?.error_for_status()?;
    if let Some(file_size) = response.content_length() {
        let file_size: usize = file_size.try_into()
            .map_err(|_| FetchError::ResponseTooLarge)?;
        if file_size > file_max_size {
            return Err(FetchError::ResponseTooLarge);
        };
    };
    let maybe_content_type_header = response.headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_string());
    let file_data = limited_response(response, file_max_size)
        .await?
        .ok_or(FetchError::ResponseTooLarge)?;
    let file_size = file_data.len();
    let media_type = get_media_type(
        &file_data,
        maybe_content_type_header.as_deref(),
        expected_media_type,
    );
    if !allowed_media_types.contains(&media_type.as_str()) {
        return Err(FetchError::UnsupportedMediaType(media_type));
    };
    Ok((file_data.into(), file_size, media_type))
}

/// Fetches arbitrary JSON data (unsigned request)
pub async fn fetch_json<T: DeserializeOwned>(
    agent: &FederationAgent,
    url: &str,
    query: &[(&str, &str)],
) -> Result<T, FetchError> {
    let http_client = build_fetcher_client(agent, url)?;
    let request_builder =
        build_request(agent, http_client, Method::GET, url);
    let response = request_builder
        .query(query)
        .send()
        .await?
        .error_for_status()?;
    let data = limited_response(response, RESPONSE_SIZE_LIMIT)
        .await?
        .ok_or(FetchError::ResponseTooLarge)?;
    let object: T = serde_json::from_slice(&data)?;
    Ok(object)
}
