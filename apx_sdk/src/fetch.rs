//! Retrieving objects or media.

use http_body_util::{
    combinators::MapErr,
    BodyDataStream,
    BodyExt,
    Limited,
};
use reqwest::{
    header,
    Body,
    Client,
    Method,
    RequestBuilder,
    StatusCode,
    Url,
};
use serde_json::{Value as JsonValue};

use apx_core::{
    http_signatures::create::{
        create_http_signature_cavage,
        HttpSignatureError,
    },
    http_types::{Method as HttpMethod},
    http_url::is_same_http_origin,
    media_type::sniff_media_type,
    url::canonical::is_same_origin,
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
    utils::extract_media_type,
};

const APPLICATION_OCTET_STREAM: &str = "application/octet-stream";

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

    #[error("stream error: {0}")]
    StreamError(String),

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

/// Returns next URL in redirection chain
fn get_target_url(
    current_url: &Url,
    location: &str, // "Location" header value
) -> Result<Url, String> {
    // https://github.com/seanmonstar/reqwest/blob/37074368012ce42e61e5649c2fffcf8c8a979e1e/src/async_impl/client.rs#L2745
    let mut next_url = current_url.join(location)
        .map_err(|error| error.to_string())?;
    if next_url.fragment().is_none() {
        // Redirection inherits the original reference's fragment, if any
        // https://www.rfc-editor.org/rfc/rfc9110#section-10.2.2
        next_url.set_fragment(current_url.fragment());
    };
    Ok(next_url)
}

fn extract_fragment(
    document: &JsonValue,
    fragment_id: &str, // fully qualified fragment ID
) -> Option<JsonValue> {
    if let Some(map) = document.as_object() {
        for (key, value) in map.iter() {
            if key == "id" && value.as_str() == Some(fragment_id) {
                return Some(document.clone());
            };
            if let Some(fragment) = extract_fragment(value, fragment_id) {
                return Some(fragment);
            };
        };
    };
    None
}

#[derive(Default)]
pub struct FetchObjectOptions {
    /// Skip origin and content type checks?
    pub skip_verification: bool,
    /// List of trusted origins for a FEP-ef61 collection
    pub fep_ef61_trusted_origins: Vec<String>,
}

/// Sends GET request to fetch ActivityPub object. Supports fragment resolution.
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
    let mut target_url = object_id.to_owned();
    let response = loop {
        if agent.ssrf_protection_enabled {
            require_safe_url(&target_url)?;
        };
        let mut request_builder =
            build_request(agent, &http_client, Method::GET, &target_url)
                .header(header::ACCEPT, AP_MEDIA_TYPE);

        if let Some(ref signer) = agent.signer {
            // Only public instances can send signed requests
            let headers = create_http_signature_cavage(
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
            .and_then(|location| get_target_url(response.url(), location).ok())
            .ok_or(FetchError::RedirectionError)?
            .to_string();
    };

    let object_location = response.url().clone();
    let content_type = response.headers()
        .get(header::CONTENT_TYPE)
        .and_then(extract_media_type)
        .unwrap_or_default();

    let object_bytes = limited_response(response, agent.response_size_limit)
        .await
        .ok_or(FetchError::ResponseTooLarge)?;
    let object_json: JsonValue = serde_json::from_slice(&object_bytes)?;
    let object_id = object_json["id"].as_str()
        .ok_or(FetchError::NoObjectId(object_location.to_string()))?
        .to_string();
    let object_json = if let Some(fragment_id) = object_location.fragment() {
        // Resolve fragment
        // https://www.w3.org/TR/cid/#fragment-resolution
        let fully_qualified_fragment_id = format!("{object_id}#{fragment_id}");
        extract_fragment(&object_json, &fully_qualified_fragment_id)
            .ok_or(FetchError::NotFound(object_location.to_string()))?
    } else {
        object_json
    };

    if options.skip_verification {
        return Ok(object_json);
    };

    // Perform authentication
    match verify_portable_object(&object_json) {
        Ok(_) => (),
        Err(AuthenticationError::InvalidObjectID(_)) => {
            return Err(FetchError::UrlError);
        },
        Err(AuthenticationError::NotPortable) => {
            // Verify authority if object is not portable
            let is_trusted = is_same_origin(object_location.as_str(), &object_id)
                .unwrap_or(false);
            if !is_trusted {
                return Err(FetchError::UnexpectedObjectId(object_location.to_string()));
            };
        },
        Err(AuthenticationError::NoProof) => {
            let is_trusted = options.fep_ef61_trusted_origins
                .iter()
                .any(|origin| {
                    is_same_http_origin(object_location.as_str(), origin)
                        .unwrap_or(false)
                });
            if !is_trusted {
                return Err(FetchError::UnexpectedObjectId(object_location.to_string()));
            };
        },
        Err(_) => return Err(FetchError::InvalidProof),
    };

    // Verify object is not a malicious upload
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
    let response = request_builder.send().await?.error_for_status()?;
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
    let file_data = limited_response(response, file_size_limit)
        .await
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

#[allow(impl_trait_overcaptures)]
pub async fn fetch_file_streaming(
    agent: &FederationAgent,
    url: &str,
    allowed_media_types: &[&str],
    file_size_limit: usize,
) ->
    Result<
        (BodyDataStream<MapErr<
            Limited<Body>,
            impl FnMut(<Limited<Body> as http_body::Body>::Error) -> FetchError
        >>, String),
        FetchError
    >
{
    if agent.ssrf_protection_enabled {
        require_safe_url(url)?;
    };
    // Redirects are allowed
    let http_client = build_fetcher_client(agent, url, false)?;
    let request_builder =
        build_request(agent, &http_client, Method::GET, url);
    let response = request_builder.send().await?.error_for_status()?;
    let media_type = response.headers()
        .get(header::CONTENT_TYPE)
        .and_then(extract_media_type)
        .unwrap_or(APPLICATION_OCTET_STREAM.to_owned());
    if !allowed_media_types.contains(&media_type.as_str()) {
        return Err(FetchError::UnexpectedContentType(media_type));
    };
    let stream = Limited::new(Body::from(response), file_size_limit)
        .map_err(|error| FetchError::StreamError(error.to_string()))
        .into_data_stream();
    Ok((stream, media_type))
}

/// Fetches arbitrary JSON data (unsigned request)
pub async fn fetch_json(
    agent: &FederationAgent,
    url: &str,
    query: &[(&str, &str)],
    accept: Option<&str>,
) -> Result<JsonValue, FetchError> {
    const APPLICATION_JSON: &str = "application/json";
    if agent.ssrf_protection_enabled {
        require_safe_url(url)?;
    };
    // Redirects are allowed
    let http_client = build_fetcher_client(agent, url, false)?;
    let request_builder =
        build_request(agent, &http_client, Method::GET, url);
    let response = request_builder
        .query(query)
        .header(header::ACCEPT, accept.unwrap_or(APPLICATION_JSON))
        .send()
        .await?
        .error_for_status()?;
    let data = limited_response(response, agent.response_size_limit)
        .await
        .ok_or(FetchError::ResponseTooLarge)?;
    let object_json = serde_json::from_slice(&data)?;
    Ok(object_json)
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use super::*;

    #[test]
    fn test_get_target_url() {
        let current_url = Url::parse("https://social.example/users/1").unwrap();
        let location = "https://social.example/actors/1";
        let target_url = get_target_url(&current_url, location).unwrap();
        assert_eq!(
            target_url.to_string(),
            "https://social.example/actors/1",
        );
    }

    #[test]
    fn test_get_target_url_inherit_fragment() {
        let current_url = Url::parse("https://social.example/users/1#main-key").unwrap();
        let location = "/actors/1";
        let target_url = get_target_url(&current_url, location).unwrap();
        assert_eq!(
            target_url.to_string(),
            "https://social.example/actors/1#main-key",
        );
    }

    #[test]
    fn test_extract_fragment() {
        let document = json!({
            "id": "https://social.example/users/1",
            "preferredUsername": "test",
            "publicKey": {
                "id": "https://social.example/users/1#main-key",
                "owner": "https://social.example/users/1",
            },
        });
        let maybe_fragment = extract_fragment(
            &document,
            "https://social.example/users/1#main-key",
        );
        assert_eq!(
            maybe_fragment.unwrap(),
            json!({
                "id": "https://social.example/users/1#main-key",
                "owner": "https://social.example/users/1",
            }),
        );
    }

    #[test]
    fn test_extract_fragment_not_found() {
        let document = json!({
            "id": "https://social.example/users/1",
            "preferredUsername": "test",
            "publicKey": {
                "id": "https://social.example/users/1#main-key",
            },
        });
        let maybe_fragment = extract_fragment(
            &document,
            "https://social.example/users/1#secondary-key",
        );
        assert_eq!(maybe_fragment.is_none(), true);
    }
}
