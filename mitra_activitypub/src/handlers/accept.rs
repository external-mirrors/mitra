use apx_core::caip10::AccountId;
use apx_sdk::deserialization::deserialize_into_object_id;
use serde::Deserialize;
use serde_json::{Value as JsonValue};

use mitra_config::Config;
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    invoices::helpers::remote_invoice_opened,
    invoices::queries::get_invoice_by_id,
    profiles::queries::{
        get_profile_by_id,
        get_remote_profile_by_actor_id,
    },
    relationships::{
        queries::{
            follow_request_accepted,
            get_follow_request_by_id,
            get_follow_request_by_remote_activity_id,
        },
        types::{DbFollowRequest, FollowRequestStatus},
    },
};
use mitra_validators::{
    activitypub::validate_object_id,
    errors::ValidationError,
};

use crate::{
    c2s::followers::add_follower,
    identifiers::{canonicalize_id, parse_local_activity_id},
    vocabulary::{FOLLOW, OFFER},
};

use super::{
    agreement::Agreement,
    Descriptor,
    HandlerResult,
};

pub async fn get_follow_request_by_activity_id(
    db_client: &impl DatabaseClient,
    instance_url: &str,
    activity_id: &str,
) -> Result<DbFollowRequest, DatabaseError> {
    match parse_local_activity_id(
        instance_url,
        activity_id,
    ) {
        Ok(follow_request_id) => {
            get_follow_request_by_id(db_client, follow_request_id).await
        },
        Err(_) => {
            get_follow_request_by_remote_activity_id(db_client, activity_id).await
        },
    }
}

#[derive(Deserialize)]
struct Accept {
    #[serde(deserialize_with = "deserialize_into_object_id")]
    actor: String,
    #[serde(deserialize_with = "deserialize_into_object_id")]
    object: String,
    result: Option<JsonValue>,
}

pub async fn handle_accept(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    activity: JsonValue,
) -> HandlerResult {
    let accept: Accept = serde_json::from_value(activity)?;
    if accept.result.is_some() {
        // Accept(Offer)
        return handle_accept_offer(config, db_client, accept).await;
    };
    // Accept(Follow)
    let canonical_actor_id = canonicalize_id(&accept.actor)?;
    let actor_profile = get_remote_profile_by_actor_id(
        db_client,
        &canonical_actor_id.to_string(),
    ).await?;
    let canonical_object_id = canonicalize_id(&accept.object)?;
    let follow_request = get_follow_request_by_activity_id(
        db_client,
        &config.instance_url(),
        &canonical_object_id.to_string(),
    ).await?;
    if follow_request.target_id != actor_profile.id {
        return Err(ValidationError("actor is not a target").into());
    };
    if matches!(follow_request.request_status, FollowRequestStatus::Accepted) {
        // Ignore Accept if follow request already accepted
        return Ok(None);
    };
    follow_request_accepted(db_client, follow_request.id).await?;
    if actor_profile.has_portable_account() {
        let source = get_profile_by_id(db_client, follow_request.source_id).await?;
        add_follower(db_client, &source, &actor_profile).await?;
    };
    Ok(Some(Descriptor::object(FOLLOW)))
}

async fn handle_accept_offer(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    accept: Accept,
) -> HandlerResult {
    let actor_profile = get_remote_profile_by_actor_id(
        db_client,
        &accept.actor,
    ).await?;
    let invoice_id = parse_local_activity_id(
        &config.instance_url(),
        &accept.object,
    )?;
    let invoice = get_invoice_by_id(db_client, invoice_id).await?;
    if invoice.recipient_id != actor_profile.id {
        return Err(ValidationError("actor is not a recipient").into());
    };
    let agreement_value = accept.result.expect("result should be present");
    let agreement: Agreement = serde_json::from_value(agreement_value)?;
    let agreement_id = agreement.id.as_ref()
        .ok_or(ValidationError("missing 'id' field"))?;
    let invoice_amount: i64 = agreement.reciprocal_commitment()
        .resource_quantity
        .parse_currency_amount()?;
    if invoice_amount != invoice.amount {
        return Err(ValidationError("unexpected amount").into());
    };
    let payment_uri = agreement.url.map(|link| link.href)
        .ok_or(ValidationError("missing 'url' field"))?;
    let account_id = AccountId::from_uri(&payment_uri)
        .map_err(|_| ValidationError("invalid account ID"))?;
    if account_id.chain_id != *invoice.chain_id.inner() {
        return Err(ValidationError("unexpected chain ID").into());
    };
    validate_object_id(agreement_id)?;
    remote_invoice_opened(
        db_client,
        invoice.id,
        &account_id.address,
        agreement_id,
    ).await?;
    Ok(Some(Descriptor::object(OFFER)))
}
