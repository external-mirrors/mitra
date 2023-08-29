use std::cmp::Ordering;

use mitra_config::{EthereumConfig, Instance};
use mitra_models::{
    database::{
        get_database_client,
        DatabaseError,
        DbPool,
    },
    profiles::queries::search_profiles_by_wallet_address,
    subscriptions::queries::{
        create_subscription,
        update_subscription,
        get_subscription_by_participants,
    },
    users::queries::get_user_by_public_wallet_address,
};
use mitra_services::ethereum::{
    subscriptions::{get_subscription_events, SubscriptionEvent},
    sync::{get_blockchain_tip, SyncState},
    EthereumApi,
    EthereumContract,
};
use mitra_utils::currencies::Currency;

use super::common::send_subscription_notifications;
use super::errors::PaymentError;

const ETHEREUM: Currency = Currency::Ethereum;

pub async fn check_ethereum_subscriptions(
    config: &EthereumConfig,
    instance: &Instance,
    web3: &EthereumApi,
    contract: &EthereumContract,
    sync_state: &mut SyncState,
    db_pool: &DbPool,
) -> Result<(), PaymentError> {
    let db_client = &mut **get_database_client(db_pool).await?;
    let (from_block, to_block) = sync_state.get_scan_range(
        &contract.address(),
        get_blockchain_tip(web3).await?,
    );
    let events = get_subscription_events(
        web3,
        contract,
        from_block,
        to_block,
    ).await?;
    for SubscriptionEvent {
        sender_address,
        recipient_address,
        expires_at,
        block_date,
    } in events {
        let profiles = search_profiles_by_wallet_address(
            db_client,
            &ETHEREUM,
            &sender_address,
            true, // prefer verified addresses
        ).await?;
        let sender = match &profiles[..] {
            [profile] => profile,
            [] => {
                // Profile not found, skip event
                log::error!("unknown subscriber {}", sender_address);
                continue;
            },
            _ => {
                // Ambiguous results, skip event
                log::error!(
                    "search returned multiple results for address {}",
                    sender_address,
                );
                continue;
            },
        };
        let recipient = get_user_by_public_wallet_address(
            db_client,
            &ETHEREUM,
            &recipient_address,
        ).await?;

        match get_subscription_by_participants(
            db_client,
            &sender.id,
            &recipient.id,
        ).await {
            Ok(subscription) => {
                if subscription.chain_id != config.chain_id {
                    // Reset is required (mitractl reset-subscriptions).
                    // Without this precaution, sender_address can be
                    // lost during the switch, leading to a loss
                    // of the ability to call withdrawReceived()
                    // from a client.
                    // See also: ApiSubscription type.
                    log::error!("can't switch to another chain");
                    continue;
                };
                let current_sender_address =
                    subscription.sender_address.unwrap_or("''".to_string());
                if current_sender_address != sender_address {
                    // Trust only key/address that was linked to profile
                    // when first subscription event occured.
                    // Key rotation is not supported.
                    log::error!(
                        "subscriber address changed from {} to {}",
                        current_sender_address,
                        sender_address,
                    );
                    continue;
                };
                if subscription.updated_at >= block_date {
                    // Event already processed
                    continue;
                };
                // Update subscription expiration date
                update_subscription(
                    db_client,
                    subscription.id,
                    &expires_at,
                    &block_date,
                ).await?;
                match expires_at.cmp(&subscription.expires_at) {
                    Ordering::Greater => {
                        log::info!(
                            "subscription extended: {0} to {1}",
                            subscription.sender_id,
                            subscription.recipient_id,
                        );
                    },
                    Ordering::Less => {
                        log::info!(
                            "subscription cancelled: {0} to {1}",
                            subscription.sender_id,
                            subscription.recipient_id,
                        );
                        continue;
                    },
                    Ordering::Equal => continue, // unchanged
                };
            },
            Err(DatabaseError::NotFound(_)) => {
                // New subscription
                create_subscription(
                    db_client,
                    &sender.id,
                    Some(&sender_address),
                    &recipient.id,
                    &config.chain_id,
                    &expires_at,
                    &block_date,
                ).await?;
                log::info!(
                    "subscription created: {0} to {1}",
                    sender.id,
                    recipient.id,
                );
            },
            Err(other_error) => return Err(other_error.into()),
        };
        send_subscription_notifications(
            db_client,
            instance,
            sender,
            &recipient,
            expires_at,
        ).await?;
    };

    sync_state.update(db_client, &contract.address(), to_block).await?;
    Ok(())
}
