use chrono::{DateTime, Utc};
use uuid::Uuid;

use mitra_activitypub::{
    builders::{
        add_person::prepare_add_person,
        remove_person::prepare_remove_person,
    },
    identifiers::LocalActorCollection,
};
use mitra_config::Instance;
use mitra_models::{
    database::{
        get_database_client,
        DatabaseClient,
        DatabaseConnectionPool,
        DatabaseError,
    },
    notifications::helpers::{
        create_subscriber_leaving_notification,
        create_subscriber_payment_notification,
        create_subscription_expiration_notification,
    },
    profiles::queries::get_profile_by_id,
    profiles::types::DbActorProfile,
    relationships::queries::unsubscribe,
    subscriptions::queries::get_expired_subscriptions,
    users::queries::get_user_by_id,
    users::types::User,
};

pub async fn send_subscription_notifications(
    db_client: &impl DatabaseClient,
    instance: &Instance,
    sender: &DbActorProfile,
    recipient: &User,
    subscription_expires_at: DateTime<Utc>,
    maybe_invoice_id: Option<Uuid>,
) -> Result<(), DatabaseError> {
    if maybe_invoice_id.is_some() {
        // Create notification only if payment was made
        create_subscriber_payment_notification(
            db_client,
            sender.id,
            recipient.id,
        ).await?;
    };
    if let Some(ref remote_sender) = sender.actor_json {
        prepare_add_person(
            instance,
            recipient,
            remote_sender,
            LocalActorCollection::Subscribers,
            subscription_expires_at,
            maybe_invoice_id,
        ).save_and_enqueue(db_client).await?;
    };
    Ok(())
}

pub async fn update_expired_subscriptions(
    instance: &Instance,
    db_pool: &DatabaseConnectionPool,
) -> Result<(), DatabaseError> {
    let db_client = &mut **get_database_client(db_pool).await?;
    for subscription in get_expired_subscriptions(db_client).await? {
        // Remove relationship
        unsubscribe(db_client, subscription.sender_id, subscription.recipient_id).await?;
        let sender = get_profile_by_id(db_client, &subscription.sender_id).await?;
        let recipient = get_user_by_id(db_client, &subscription.recipient_id).await?;
        log::info!(
            "subscription expired: {0} to {1}",
            sender,
            recipient,
        );
        if let Some(ref remote_sender) = sender.actor_json {
            prepare_remove_person(
                instance,
                &recipient,
                remote_sender,
                LocalActorCollection::Subscribers,
            ).save_and_enqueue(db_client).await?;
        } else {
            create_subscription_expiration_notification(
                db_client,
                subscription.recipient_id,
                subscription.sender_id,
            ).await?;
        };
        create_subscriber_leaving_notification(
            db_client,
            sender.id,
            recipient.id,
        ).await?;
    };
    Ok(())
}
