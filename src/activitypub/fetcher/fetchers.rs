use bytes::Bytes;
use reqwest::{Client, Method, RequestBuilder, StatusCode};
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::{Value as JsonValue};

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
    agent::FederationAgent,
    constants::{AP_CONTEXT, AP_MEDIA_TYPE},
    http_client::{
        build_http_client,
        get_network_type,
        limited_response,
        RESPONSE_SIZE_LIMIT,
    },
    vocabulary::GROUP,
};
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

    #[error("unsupported media type: {0}")]
    UnsupportedMediaType(String),

    #[error("too many objects")]
    RecursionError,

    #[error("{0}")]
    OtherError(&'static str),
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
            "",
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

impl JsonResourceDescriptor {
    fn find_actor_id(&self) -> Option<String> {
        // Lemmy servers can have Group and Person actors with the same name
        // https://github.com/LemmyNet/lemmy/issues/2037
        let ap_type_property = format!("{}#type", AP_CONTEXT);
        let group_link = self.links.iter()
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
            self.links.iter().find(|link| link.rel == "self")?
        };
        let actor_id = link.href.as_ref()?.to_string();
        Some(actor_id)
    }
}

pub async fn perform_webfinger_query(
    agent: &FederationAgent,
    actor_address: &ActorAddress,
) -> Result<String, FetchError> {
    let webfinger_resource = actor_address.to_acct_uri();
    let webfinger_url = format!(
        "{}://{}/.well-known/webfinger",
        guess_protocol(&actor_address.hostname),
        actor_address.hostname,
    );
    let http_client = build_fetcher_client(agent, &webfinger_url)?;
    let request_builder =
        build_request(agent, http_client, Method::GET, &webfinger_url);
    let response = request_builder
        .query(&[("resource", webfinger_resource)])
        .send().await?
        .error_for_status()?;
    let webfinger_data = limited_response(response, RESPONSE_SIZE_LIMIT)
        .await?
        .ok_or(FetchError::ResponseTooLarge)?;
    let jrd: JsonResourceDescriptor =
        serde_json::from_slice(&webfinger_data)?;
    let actor_id = jrd.find_actor_id()
        .ok_or(FetchError::OtherError("actor ID not found"))?;
    Ok(actor_id)
}

pub async fn fetch_actor(
    agent: &FederationAgent,
    actor_url: &str,
) -> Result<Actor, FetchError> {
    let actor: Actor = fetch_object(agent, actor_url).await?;
    if actor.id != actor_url {
        log::warn!("redirected from {} to {}", actor_url, actor.id);
    };
    Ok(actor)
}

pub async fn fetch_collection(
    agent: &FederationAgent,
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
        fetch_object(agent, collection_url).await?;
    let mut items = collection.ordered_items;
    if let Some(first_page_value) = collection.first {
        let page: CollectionPage = match first_page_value {
            JsonValue::String(first_page_url) => {
                fetch_object(agent, &first_page_url).await?
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

#[cfg(test)]
mod tests {
    use crate::webfinger::types::Link;
    use super::*;

    #[test]
    fn test_jrd_find_actor_id() {
        let actor_id = "https://social.example/users/test";
        let link_actor = Link {
            rel: "self".to_string(),
            media_type: Some("application/activity+json".to_string()),
            href: Some(actor_id.to_string()),
            properties: Default::default(),
        };
        let jrd = JsonResourceDescriptor {
            subject: "acct:test@social.example".to_string(),
            links: vec![link_actor],
        };
        assert_eq!(jrd.find_actor_id().unwrap(), actor_id);
    }
}
