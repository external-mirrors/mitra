use std::path::Path;

use bytes::Bytes;
use reqwest::{Client, Method, RequestBuilder, StatusCode};
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::{Value as JsonValue};

use mitra_config::Instance;
use mitra_models::profiles::types::PublicKeyType;
use mitra_utils::{
    files::sniff_media_type,
    http_signatures::create::{
        create_http_signature,
        HttpSignatureError,
    },
    urls::{guess_protocol, is_safe_url},
};

use crate::activitypub::{
    actors::types::Actor,
    constants::{AP_CONTEXT, AP_MEDIA_TYPE},
    http_client::{
        build_federation_client,
        get_network_type,
        limited_response,
        RESPONSE_SIZE_LIMIT,
    },
    identifiers::{local_actor_key_id, local_instance_actor_id},
    vocabulary::GROUP,
};
use crate::media::{save_file, SUPPORTED_MEDIA_TYPES};
use crate::webfinger::types::{ActorAddress, JsonResourceDescriptor};

#[derive(thiserror::Error, Debug)]
pub enum FetchError {
    #[error(transparent)]
    SignatureError(#[from] HttpSignatureError),

    #[error("inavlid URL")]
    UrlError(#[from] url::ParseError),

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

    #[error(transparent)]
    FileError(#[from] std::io::Error),

    #[error("unsupported media type: {0}")]
    UnsupportedMediaType(String),

    #[error("too many objects")]
    RecursionError,

    #[error("{0}")]
    OtherError(&'static str),
}

fn build_client(
    instance: &Instance,
    request_url: &str,
) -> Result<Client, FetchError> {
    let network = get_network_type(request_url)?;
    let client = build_federation_client(
        instance,
        network,
        instance.fetcher_timeout,
    )?;
    Ok(client)
}

fn build_request(
    instance: &Instance,
    client: Client,
    method: Method,
    url: &str,
) -> RequestBuilder {
    let mut request_builder = client.request(method, url);
    if !instance.is_private {
        // Public instances should set User-Agent header
        request_builder = request_builder
            .header(reqwest::header::USER_AGENT, instance.agent());
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
    instance: &Instance,
    url: &str,
) -> Result<Bytes, FetchError> {
    let client = build_client(instance, url)?;
    let mut request_builder = build_request(instance, client, Method::GET, url)
        .header(reqwest::header::ACCEPT, AP_MEDIA_TYPE);

    if !instance.is_private {
        // Only public instances can send signed requests
        let instance_actor_id = local_instance_actor_id(&instance.url());
        let instance_actor_key_id = local_actor_key_id(
            &instance_actor_id,
            PublicKeyType::RsaPkcs1,
        );
        let headers = create_http_signature(
            Method::GET,
            url,
            "",
            &instance.actor_key,
            &instance_actor_key_id,
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
    instance: &Instance,
    object_url: &str,
) -> Result<T, FetchError> {
    let object_json = send_request(instance, object_url).await?;
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
    instance: &Instance,
    url: &str,
    expected_media_type: Option<&str>,
    file_max_size: usize,
    output_dir: &Path,
) -> Result<(String, usize, String), FetchError> {
    if !is_safe_url(url) {
        return Err(FetchError::UnsafeUrl);
    };
    let client = build_client(instance, url)?;
    let request_builder =
        build_request(instance, client, Method::GET, url);
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
    if !SUPPORTED_MEDIA_TYPES.contains(&media_type.as_str()) {
        return Err(FetchError::UnsupportedMediaType(media_type));
    };
    let file_name = save_file(
        file_data.to_vec(),
        output_dir,
        Some(&media_type),
    )?;
    Ok((file_name, file_size, media_type))
}

pub async fn perform_webfinger_query(
    instance: &Instance,
    actor_address: &ActorAddress,
) -> Result<String, FetchError> {
    let webfinger_account_uri = format!("acct:{}", actor_address);
    let webfinger_url = format!(
        "{}://{}/.well-known/webfinger",
        guess_protocol(&actor_address.hostname),
        actor_address.hostname,
    );
    let client = build_client(instance, &webfinger_url)?;
    let request_builder =
        build_request(instance, client, Method::GET, &webfinger_url);
    let response = request_builder
        .query(&[("resource", webfinger_account_uri)])
        .send().await?
        .error_for_status()?;
    let webfinger_data = limited_response(response, RESPONSE_SIZE_LIMIT)
        .await?
        .ok_or(FetchError::ResponseTooLarge)?;
    let jrd: JsonResourceDescriptor =
        serde_json::from_slice(&webfinger_data)?;
    // Lemmy servers can have Group and Person actors with the same name
    // https://github.com/LemmyNet/lemmy/issues/2037
    let ap_type_property = format!("{}#type", AP_CONTEXT);
    let group_link = jrd.links.iter()
        .find(|link| {
            link.rel == "self" &&
            link.properties
                .get(&ap_type_property)
                .map(|val| val.as_str()) == Some(GROUP)
        });
    let link = if let Some(link) = group_link {
        // Prefer Group if the actor type is provided
        link
    } else {
        // Otherwise take first "self" link
        jrd.links.iter()
            .find(|link| link.rel == "self")
            .ok_or(FetchError::OtherError("self link not found"))?
    };
    let actor_url = link.href.as_ref()
        .ok_or(FetchError::OtherError("account href not found"))?
        .to_string();
    Ok(actor_url)
}

pub async fn fetch_actor(
    instance: &Instance,
    actor_url: &str,
) -> Result<Actor, FetchError> {
    let actor: Actor = fetch_object(instance, actor_url).await?;
    if actor.id != actor_url {
        log::warn!("redirected from {} to {}", actor_url, actor.id);
    };
    Ok(actor)
}

pub async fn fetch_collection(
    instance: &Instance,
    collection_url: &str,
    limit: usize,
) -> Result<Vec<JsonValue>, FetchError> {
    // https://www.w3.org/TR/activitystreams-core/#collections
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Collection {
        first: Option<JsonValue>, // page can be embedded
        #[serde(default)]
        ordered_items: Vec<JsonValue>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct CollectionPage {
        #[serde(default)]
        ordered_items: Vec<JsonValue>,
    }

    let collection: Collection =
        fetch_object(instance, collection_url).await?;
    let mut items = collection.ordered_items;
    if let Some(first_page_value) = collection.first {
        let page: CollectionPage = match first_page_value {
            JsonValue::String(first_page_url) => {
                fetch_object(instance, &first_page_url).await?
            },
            _ => serde_json::from_value(first_page_value)?,
        };
        items.extend(page.ordered_items);
    };
    let activities = items.into_iter()
        .take(limit)
        .collect();
    Ok(activities)
}
