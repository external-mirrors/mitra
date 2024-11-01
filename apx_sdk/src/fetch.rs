use http::header;
use reqwest::{Client, Method, RequestBuilder, StatusCode};
use serde::de::DeserializeOwned;
use serde_json::{Value as JsonValue};

use apx_core::{
    http_signatures::create::{
        create_http_signature,
        HttpSignatureError,
    },
    media_type::sniff_media_type,
};

use super::{
    agent::FederationAgent,
    authentication::{
        verify_portable_object,
        AuthenticationError,
    },
    constants::{AP_MEDIA_TYPE, AS_MEDIA_TYPE},
    http_client::{
        build_http_client,
        get_network_type,
        limited_response,
        require_safe_url,
        UnsafeUrlError,
        REDIRECT_LIMIT,
    },
    utils::{extract_media_type, is_same_hostname},
};

#[derive(thiserror::Error, Debug)]
pub enum FetchError {
    #[error(transparent)]
    SignatureError(#[from] HttpSignatureError),

    #[error("inavlid URL")]
    UrlError,

    #[error(transparent)]
    UnsafeUrl(#[from] UnsafeUrlError),

    #[error(transparent)]
    RequestError(#[from] reqwest::Error),

    #[error("access denied: {0}")]
    Forbidden(String),

    #[error("resource not found: {0}")]
    NotFound(String),

    #[error("redirection error")]
    RedirectionError,

    #[error("response size exceeds limit")]
    ResponseTooLarge,

    #[error("json parse error: {0}")]
    JsonParseError(#[from] serde_json::Error),

    #[error("unexpected content type: {0}")]
    UnexpectedContentType(String),

    #[error("object without ID at {0}")]
    NoObjectId(String),

    #[error("unexpected object ID at {0}")]
    UnexpectedObjectId(String),

    #[error("invalid proof")]
    InvalidProof,

    #[error("too many objects")]
    RecursionError,

    #[error("gateways are not provided")]
    NoGateway,
}

fn build_fetcher_client(
    agent: &FederationAgent,
    request_url: &str,
    no_redirect: bool,
) -> Result<Client, FetchError> {
    let network = get_network_type(request_url)
        .map_err(|_| FetchError::UrlError)?;
    let http_client = build_http_client(
        agent,
        network,
        agent.fetcher_timeout,
        no_redirect,
    )?;
    Ok(http_client)
}

fn build_request(
    agent: &FederationAgent,
    http_client: &Client,
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
        (Some(url), Some(StatusCode::FORBIDDEN)) => {
            FetchError::Forbidden(url.to_string())
        },
        (Some(url), Some(StatusCode::NOT_FOUND)) => {
            FetchError::NotFound(url.to_string())
        },
        _ => error.into(),
    }
}

/// Sends GET request to fetch AP object
pub async fn fetch_object(
    agent: &FederationAgent,
    object_id: &str,
    allow_fep_ef61_noproof: bool,
) -> Result<JsonValue, FetchError> {
    // Don't follow redirects automatically,
    // because request needs to be signed again after every redirect
    let http_client = build_fetcher_client(
        agent,
        object_id,
        true,
    )?;

    let mut redirect_count = 0;
    let mut target_url = object_id.to_string();
    let mut response = loop {
        if agent.ssrf_protection_enabled {
            require_safe_url(&target_url)?;
        };
        let mut request_builder =
            build_request(agent, &http_client, Method::GET, &target_url)
                .header(header::ACCEPT, AP_MEDIA_TYPE);

        if !agent.is_instance_private {
            // Only public instances can send signed requests
            let headers = create_http_signature(
                Method::GET,
                &target_url,
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
        if !response.status().is_redirection() {
            break response;
        };
        // Redirected
        redirect_count += 1;
        if redirect_count >= REDIRECT_LIMIT {
            return Err(FetchError::RedirectionError);
        };
        target_url = response.headers()
            .get(header::LOCATION)
            .ok_or(FetchError::RedirectionError)?
            .to_str()
            .map_err(|_| FetchError::RedirectionError)?
            .to_string();
    };

    let data = limited_response(&mut response, agent.response_size_limit)
        .await?
        .ok_or(FetchError::ResponseTooLarge)?;

    let object_json: JsonValue = serde_json::from_slice(&data)?;
    let object_location = response.url().as_str();
    let object_id = object_json["id"].as_str()
        .ok_or(FetchError::NoObjectId(object_location.to_string()))?;

    // Perform authentication
    match verify_portable_object(&object_json) {
        Ok(_) => (),
        Err(AuthenticationError::InvalidObjectID(_)) => {
            return Err(FetchError::UrlError);
        },
        Err(AuthenticationError::NotPortable) => {
            // Verify authority if object is not portable
            let is_same_origin = is_same_hostname(object_id, object_location)
                .unwrap_or(false);
            if !is_same_origin {
                return Err(FetchError::UnexpectedObjectId(object_location.to_string()));
            };
        },
        Err(AuthenticationError::NoProof) if allow_fep_ef61_noproof => {
            // Fallback to authority check
            let is_same_authority = is_same_hostname(object_id, object_location)
                .unwrap_or(false);
            if !is_same_authority {
                return Err(FetchError::UnexpectedObjectId(object_location.to_string()));
            };
        },
        Err(_) => return Err(FetchError::InvalidProof),
    };

    // Verify object is not a malicious upload
    let content_type = response.headers()
        .get(header::CONTENT_TYPE)
        .and_then(extract_media_type)
        .unwrap_or_default();
    const ALLOWED_TYPES: [&str; 3] = [
        AP_MEDIA_TYPE,
        AS_MEDIA_TYPE,
        "application/ld+json",
    ];
    if !ALLOWED_TYPES.contains(&content_type.as_str()) {
        return Err(FetchError::UnexpectedContentType(content_type));
    };

    Ok(object_json)
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
    file_size_limit: usize,
) -> Result<(Vec<u8>, usize, String), FetchError> {
    if agent.ssrf_protection_enabled {
        require_safe_url(url)?;
    };
    // Redirects are allowed
    let http_client = build_fetcher_client(agent, url, false)?;
    let request_builder =
        build_request(agent, &http_client, Method::GET, url);
    let mut response = request_builder.send().await?.error_for_status()?;
    if let Some(file_size) = response.content_length() {
        let file_size: usize = file_size.try_into()
            .map_err(|_| FetchError::ResponseTooLarge)?;
        if file_size > file_size_limit {
            return Err(FetchError::ResponseTooLarge);
        };
    };
    let maybe_content_type_header = response.headers()
        .get(header::CONTENT_TYPE)
        .and_then(extract_media_type);
    let file_data = limited_response(&mut response, file_size_limit)
        .await?
        .ok_or(FetchError::ResponseTooLarge)?;
    let file_size = file_data.len();
    // Content-Type header has the highest priority
    let media_type = get_media_type(
        &file_data,
        maybe_content_type_header.as_deref(),
        expected_media_type,
    );
    if !allowed_media_types.contains(&media_type.as_str()) {
        return Err(FetchError::UnexpectedContentType(media_type));
    };
    Ok((file_data.into(), file_size, media_type))
}

/// Fetches arbitrary JSON data (unsigned request)
pub async fn fetch_json<T: DeserializeOwned>(
    agent: &FederationAgent,
    url: &str,
    query: &[(&str, &str)],
) -> Result<T, FetchError> {
    if agent.ssrf_protection_enabled {
        require_safe_url(url)?;
    };
    // Redirects are allowed
    let http_client = build_fetcher_client(agent, url, false)?;
    let request_builder =
        build_request(agent, &http_client, Method::GET, url);
    let mut response = request_builder
        .query(query)
        .send()
        .await?
        .error_for_status()?;
    let data = limited_response(&mut response, agent.response_size_limit)
        .await?
        .ok_or(FetchError::ResponseTooLarge)?;
    let object: T = serde_json::from_slice(&data)?;
    Ok(object)
}
