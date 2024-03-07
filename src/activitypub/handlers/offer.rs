use serde::Deserialize;
use serde_json::{Value as JsonValue};

use mitra_activitypub::identifiers::parse_local_primary_intent_id;
use mitra_config::Config;
use mitra_models::{
    database::DatabaseClient,
    invoices::queries::create_invoice,
    profiles::queries::get_profile_by_remote_actor_id,
    profiles::types::MoneroSubscription,
    users::queries::get_user_by_name,
};
use mitra_services::monero::wallet::create_monero_address;
use mitra_validators::errors::ValidationError;

use crate::activitypub::{
    builders::accept_offer::prepare_accept_offer,
    vocabulary::AGREEMENT,
};

use super::{
    agreement::Agreement,
    HandlerError,
    HandlerResult,
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Offer {
    id: String,
    actor: String,
    object: Agreement,
}

pub async fn handle_offer(
    config: &Config,
    db_client: &impl DatabaseClient,
    activity: JsonValue,
) -> HandlerResult {
    let activity: Offer = serde_json::from_value(activity)
        .map_err(|_| ValidationError("unexpected activity structure"))?;
    let actor_profile = get_profile_by_remote_actor_id(
        db_client,
        &activity.actor,
    ).await?;
    let primary_commitment = activity.object.primary_commitment()?;
    let reciprocal_commitment = activity.object.reciprocal_commitment()?;
    let (username, chain_id) = parse_local_primary_intent_id(
        &config.instance_url(),
        &primary_commitment.satisfies,
    )?;
    let proposer = get_user_by_name(db_client, &username).await?;
    let monero_config = config.monero_config()
        .ok_or(ValidationError("recipient can't accept payment"))?;
    if chain_id != monero_config.chain_id {
        return Err(ValidationError("recipient can't accept payment").into());
    };
    let subscription_option: MoneroSubscription = proposer.profile
        .payment_options
        .find_subscription_option(&chain_id)
        .ok_or(ValidationError("recipient can't accept payment"))?;
    let duration = primary_commitment.resource_quantity
        .parse_duration()?;
    let amount: u64 = reciprocal_commitment.resource_quantity
        .parse_currency_amount()?;
    let expected_duration = amount / subscription_option.price.get();
    if duration != expected_duration {
        return Err(ValidationError("invalid duration").into());
    };
    let payment_address = create_monero_address(monero_config).await
        .map_err(|_| HandlerError::ServiceError("failed to create monero address"))?
        .to_string();
    let db_invoice = create_invoice(
        db_client,
        &actor_profile.id,
        &proposer.id,
        &subscription_option.chain_id,
        &payment_address,
        amount,
    ).await?;
    let remote_actor = actor_profile.actor_json
        .expect("actor data should be present");
    prepare_accept_offer(
        &config.instance(),
        &proposer,
        &subscription_option,
        &db_invoice,
        &remote_actor,
        &activity.id,
    )?.enqueue(db_client).await?;
    Ok(Some(AGREEMENT))
}
