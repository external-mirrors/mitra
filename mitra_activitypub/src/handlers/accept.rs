use serde::Deserialize;
use serde_json::{Value as JsonValue};

use mitra_config::Config;
use mitra_federation::deserialization::deserialize_into_object_id;
use mitra_models::{
    database::DatabaseClient,
    invoices::helpers::remote_invoice_opened,
    invoices::queries::get_invoice_by_id,
    profiles::queries::get_remote_profile_by_actor_id,
    relationships::queries::{
        follow_request_accepted,
        get_follow_request_by_id,
    },
    relationships::types::FollowRequestStatus,
};
use mitra_utils::caip10::AccountId;
use mitra_validators::{
    activitypub::validate_object_id,
    errors::ValidationError,
};

use crate::{
    identifiers::parse_local_object_id,
    vocabulary::{FOLLOW, OFFER},
};

use super::{
    agreement::Agreement,
    HandlerResult,
};

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
    let activity: Accept = serde_json::from_value(activity)
        .map_err(|_| ValidationError("unexpected activity structure"))?;
    if activity.result.is_some() {
        // Accept(Offer)
        return handle_accept_offer(config, db_client, activity).await;
    };
    // Accept(Follow)
    let actor_profile = get_remote_profile_by_actor_id(
        db_client,
        &activity.actor,
    ).await?;
    let follow_request_id = parse_local_object_id(
        &config.instance_url(),
        &activity.object,
    )?;
    let follow_request = get_follow_request_by_id(db_client, &follow_request_id).await?;
    if follow_request.target_id != actor_profile.id {
        return Err(ValidationError("actor is not a target").into());
    };
    if matches!(follow_request.request_status, FollowRequestStatus::Accepted) {
        // Ignore Accept if follow request already accepted
        return Ok(None);
    };
    follow_request_accepted(db_client, &follow_request.id).await?;
    Ok(Some(FOLLOW))
}

async fn handle_accept_offer(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    activity: Accept,
) -> HandlerResult {
    let actor_profile = get_remote_profile_by_actor_id(
        db_client,
        &activity.actor,
    ).await?;
    let invoice_id = parse_local_object_id(
        &config.instance_url(),
        &activity.object,
    )?;
    let invoice = get_invoice_by_id(db_client, &invoice_id).await?;
    if invoice.recipient_id != actor_profile.id {
        return Err(ValidationError("actor is not a recipient").into());
    };
    let agreement_value = activity.result.expect("result should be present");
    let agreement: Agreement = serde_json::from_value(agreement_value)
        .map_err(|_| ValidationError("unexpected activity structure"))?;
    let agreement_id = agreement.id.as_ref()
        .ok_or(ValidationError("missing 'id' field"))?;
    let invoice_amount: i64 = agreement.reciprocal_commitment()?
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
        &invoice.id,
        &account_id.address,
        agreement_id,
    ).await?;
    Ok(Some(OFFER))
}
