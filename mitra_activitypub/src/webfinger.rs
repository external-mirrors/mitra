use apx_sdk::{
    addresses::WebfingerAddress,
    agent::FederationAgent,
    core::url::{
        hostname::is_subdomain_of,
        http_uri::HttpUri,
    },
    fetch::fetch_json,
    jrd::{JsonResourceDescriptor, JRD_MEDIA_TYPE},
};
use serde_json::{Value as JsonValue};

use mitra_validators::errors::ValidationError;

use crate::errors::HandlerError;

pub async fn fetch_webfinger_jrd(
    agent: &FederationAgent,
    webfinger_address: &WebfingerAddress,
) -> Result<JsonValue, HandlerError> {
    let webfinger_resource = webfinger_address.to_acct_uri();
    let webfinger_uri = webfinger_address.endpoint_uri();
    let jrd_value = fetch_json(
        agent,
        &webfinger_uri,
        &[("resource", &webfinger_resource)],
        Some(JRD_MEDIA_TYPE),
    ).await?;
    Ok(jrd_value)
}

pub(super) async fn perform_webfinger_query(
    agent: &FederationAgent,
    webfinger_address: &WebfingerAddress,
) -> Result<String, HandlerError> {
    let jrd_value = fetch_webfinger_jrd(agent, webfinger_address).await?;
    let jrd: JsonResourceDescriptor = serde_json::from_value(jrd_value)?;
    let actor_id = jrd.actor_id()
        .ok_or(ValidationError("actor ID is not found in JRD"))?;
    Ok(actor_id)
}

// Discover preferred webfinger hostname
// https://swicg.github.io/activitypub-webfinger/#reverse-discovery
pub(super) async fn peform_reverse_webfinger_query(
    agent: &FederationAgent,
    username: &str,
    actor_id: &HttpUri,
) -> Result<String, HandlerError> {
    let server_hostname = actor_id.hostname().to_string();
    let address = WebfingerAddress::new_unchecked(
        username,
        &server_hostname,
    );
    let jrd_value = fetch_webfinger_jrd(agent, &address).await?;
    let jrd: JsonResourceDescriptor = serde_json::from_value(jrd_value)?;
    let discovered_address = WebfingerAddress::from_acct_uri(&jrd.subject)
        .map_err(|_| ValidationError("invalid JRD subject"))?;
    if discovered_address.username() != username {
        return Err(ValidationError("username mismatch").into());
    };
    if discovered_address.hostname() != server_hostname {
        // Validate
        if !is_subdomain_of(&server_hostname, discovered_address.hostname()) {
            return Err(ValidationError("discovered address has unexpected hostname").into());
        };
        let discovered_actor_id =
            perform_webfinger_query(agent, &discovered_address).await?;
        if discovered_actor_id != actor_id.to_string() {
            return Err(ValidationError("unexpected actor ID in JRD").into());
        };
        log::info!("discovered new webfinger address: {}", discovered_address);
    };
    Ok(discovered_address.hostname().to_owned())
}
