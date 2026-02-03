use mitra_activitypub::{
    builders::remove_person::prepare_remove_subscriber,
};
use mitra_config::Instance;
use mitra_models::{
    database::{
        get_database_client,
        DatabaseConnectionPool,
        DatabaseError,
    },
    notifications::helpers::{
        create_subscriber_leaving_notification,
        create_subscription_expiration_notification,
    },
    profiles::queries::get_profile_by_id,
    relationships::queries::unsubscribe,
    subscriptions::queries::get_expired_subscriptions,
    users::queries::get_user_by_id,
};

pub async fn update_expired_subscriptions(
    instance: &Instance,
    db_pool: &DatabaseConnectionPool,
) -> Result<(), DatabaseError> {
    let db_client = &mut **get_database_client(db_pool).await?;
    for subscription in get_expired_subscriptions(db_client).await? {
        // Remove relationship
        unsubscribe(db_client, subscription.sender_id, subscription.recipient_id).await?;
        let sender = get_profile_by_id(db_client, subscription.sender_id).await?;
        let recipient = get_user_by_id(db_client, subscription.recipient_id).await?;
        log::info!(
            "subscription expired: {0} to {1}",
            sender,
            recipient,
        );
        if let Some(ref remote_sender) = sender.actor_json {
            prepare_remove_subscriber(
                instance,
                remote_sender,
                &recipient,
            ).save_and_enqueue(db_client).await?;
        } else {
            create_subscription_expiration_notification(
                db_client,
                subscription.recipient_id,
                subscription.sender_id,
            ).await?;
        };
        if sender.is_anonymous() {
            // Don't generate notification
            continue;
        };
        create_subscriber_leaving_notification(
            db_client,
            sender.id,
            recipient.id,
        ).await?;
    };
    Ok(())
}
