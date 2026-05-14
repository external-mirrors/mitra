use serde::Deserialize;
use serde_json::{Value as JsonValue};

use mitra_adapters::payments::monero::{
    create_payment_address,
    PaymentError,
};
use mitra_config::Config;
use mitra_models::{
    database::{
        get_database_client,
        DatabaseConnectionPool,
    },
    invoices::queries::create_local_invoice,
    payment_methods::queries::get_payment_method_by_chain_id,
    profiles::queries::get_remote_profile_by_actor_id,
    profiles::types::MoneroSubscription,
    users::queries::get_user_by_name,
};
use mitra_validators::errors::ValidationError;

use crate::{
    builders::accept_offer::prepare_accept_offer,
    identifiers::parse_local_primary_intent_id,
    importers::ApClient,
    vocabulary::AGREEMENT,
};

use super::{
    agreement::Agreement,
    Descriptor,
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
    ap_client: &ApClient,
    db_pool: &DatabaseConnectionPool,
    activity: JsonValue,
) -> HandlerResult {
    let offer: Offer = serde_json::from_value(activity)?;
    let db_client = &**get_database_client(db_pool).await?;
    let actor_profile = get_remote_profile_by_actor_id(
        db_client,
        &offer.actor,
    ).await?;
    let primary_commitment = offer.object.primary_commitment();
    let reciprocal_commitment = offer.object.reciprocal_commitment();
    let (username, chain_id) = parse_local_primary_intent_id(
        ap_client.instance.uri_str(),
        &primary_commitment.satisfies,
    )?;
    let proposer = get_user_by_name(db_client, &username).await?;
    let payment_method = get_payment_method_by_chain_id(
        db_client,
        proposer.id,
        &chain_id,
    )
        .await?
        .ok_or(ValidationError("recipient can't accept payment"))?;
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
    let payment_address = create_payment_address(
        config,
        &payment_method,
    ).await.map_err(|error| match error {
        PaymentError::DatabaseError(db_error) => db_error.into(),
        _ => HandlerError::ServiceError("failed to create monero address"),
    })?;
    let db_invoice = create_local_invoice(
        db_client,
        actor_profile.id,
        proposer.id,
        payment_method.payment_type,
        payment_method.chain_id.inner(),
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
        &offer.id,
    )?.save_and_enqueue(db_client).await?;
    Ok(Some(Descriptor::object(AGREEMENT)))
}
