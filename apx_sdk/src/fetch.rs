use reqwest::{
    header,
    Client,
    Method,
    RequestBuilder,
    StatusCode,
};
use serde_json::{Value as JsonValue};

use apx_core::{
    http_signatures::create::{
        create_http_signature,
        HttpSignatureError,
    },
    http_types::{Method as HttpMethod},
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
    url::is_same_origin,
    utils::extract_media_type,
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
    if let Some(ref user_agent) = agent.user_agent {
        request_builder = request_builder
            .header(header::USER_AGENT, user_agent);
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

#[derive(Default)]
pub struct FetchObjectOptions {
    /// Skip origin and content type checks?
    pub skip_verification: bool,
    /// List of trusted origins for a FEP-ef61 collection
    pub fep_ef61_trusted_origins: Vec<String>,
}

/// Sends GET request to fetch AP object
pub async fn fetch_object(
    agent: &FederationAgent,
    object_id: &str,
    options: FetchObjectOptions,
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

        if let Some(ref signer) = agent.signer {
            // Only public instances can send signed requests
            let headers = create_http_signature(
                HttpMethod::GET,
                &target_url,
                b"",
                signer,
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
            .and_then(|location| location.to_str().ok())
            .and_then(|location| {
                // https://github.com/seanmonstar/reqwest/blob/37074368012ce42e61e5649c2fffcf8c8a979e1e/src/async_impl/client.rs#L2745
                response.url().join(location).ok()
            })
            .ok_or(FetchError::RedirectionError)?
            .to_string();
    };

    let data = limited_response(&mut response, agent.response_size_limit)
        .await?
        .ok_or(FetchError::ResponseTooLarge)?;

    let object_json: JsonValue = serde_json::from_slice(&data)?;
    if options.skip_verification {
        return Ok(object_json);
    };

    // Perform authentication
    let object_location = response.url().as_str();
    let object_id = object_json["id"].as_str()
        .ok_or(FetchError::NoObjectId(object_location.to_string()))?;

    match verify_portable_object(&object_json) {
        Ok(_) => (),
        Err(AuthenticationError::InvalidObjectID(_)) => {
            return Err(FetchError::UrlError);
        },
        Err(AuthenticationError::NotPortable) => {
            // Verify authority if object is not portable
            let is_trusted = is_same_origin(object_id, object_location)
                .unwrap_or(false);
            if !is_trusted {
                return Err(FetchError::UnexpectedObjectId(object_location.to_string()));
            };
        },
        Err(AuthenticationError::NoProof) => {
            let is_trusted = options.fep_ef61_trusted_origins
                .iter()
                .any(|origin| {
                    is_same_origin(object_location, origin).unwrap_or(false)
                });
            if !is_trusted {
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
    const APPLICATION_OCTET_STREAM: &str = "application/octet-stream";
    maybe_media_type
        .or(default_media_type)
        .map(|media_type| media_type.to_string())
        // Ignore if reported media type is application/octet-stream
        .filter(|media_type| media_type != APPLICATION_OCTET_STREAM)
        // Sniff media type if not provided
        .or(sniff_media_type(file_data))
        .unwrap_or(APPLICATION_OCTET_STREAM.to_string())
}

pub async fn fetch_file(
    agent: &FederationAgent,
    url: &str,
    expected_media_type: Option<&str>,
    allowed_media_types: &[&str],
    file_size_limit: usize,
) -> Result<(Vec<u8>, String), FetchError> {
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
    // Content-Type header has the highest priority
    let media_type = get_media_type(
        &file_data,
        maybe_content_type_header.as_deref(),
        expected_media_type,
    );
    if !allowed_media_types.contains(&media_type.as_str()) {
        return Err(FetchError::UnexpectedContentType(media_type));
    };
    Ok((file_data.into(), media_type))
}

/// Fetches arbitrary JSON data (unsigned request)
pub async fn fetch_json(
    agent: &FederationAgent,
    url: &str,
    query: &[(&str, &str)],
) -> Result<JsonValue, FetchError> {
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
    let object_json = serde_json::from_slice(&data)?;
    Ok(object_json)
}
