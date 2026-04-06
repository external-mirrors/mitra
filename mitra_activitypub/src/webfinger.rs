use apx_sdk::{
    addresses::WebfingerAddress,
    agent::FederationAgent,
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
