use std::num::NonZeroU64;

use actix_web::{
    delete,
    dev::ConnectionInfo,
    get,
    post,
    web,
    HttpResponse,
    Scope,
};
use actix_web_httpauth::extractors::bearer::BearerAuth;
use uuid::Uuid;

use mitra_activitypub::{
    builders::{
        add_person::prepare_add_subscriber,
        offer_agreement::prepare_offer_agreement,
        update_person::prepare_update_person,
    },
};
use mitra_adapters::payments::subscriptions::{
    create_or_update_local_subscription,
    validate_subscription_price,
};
use mitra_config::Config;
use mitra_models::{
    database::{get_database_client, DatabaseConnectionPool},
    invoices::queries::{
        create_local_invoice,
        create_remote_invoice,
        get_invoice_by_id,
        set_invoice_status,
    },
    invoices::types::InvoiceStatus,
    profiles::queries::{
        get_profile_by_id,
        update_profile,
    },
    profiles::types::{
        MoneroSubscription,
        PaymentOption,
        ProfileUpdateData,
        RemoteMoneroSubscription,
    },
    relationships::queries::has_relationship,
    relationships::types::RelationshipType,
    subscriptions::queries::get_subscription_by_participants,
    users::queries::get_user_by_id,
    users::types::Permission,
};
use mitra_services::{
    media::MediaServer,
    monero::{
        utils::validate_monero_address,
        wallet::create_monero_address,
    },
};
use mitra_validators::{
    invoices::validate_amount,
    errors::ValidationError,
};

use crate::http::get_request_base_url;
use crate::mastodon_api::{
    accounts::types::Account,
    auth::get_current_user,
    errors::MastodonError,
    media_server::ClientMediaServer,
};

use super::types::{
    Invoice,
    InvoiceData,
    SubscriberData,
    SubscriptionDetails,
    SubscriptionOption,
    SubscriptionQueryParams,
};

#[post("")]
async fn create_subscription_view(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
    subscriber_data: web::Json<SubscriberData>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let subscriber = get_profile_by_id(
        db_client,
        subscriber_data.subscriber_id,
    ).await?;
    let is_follower = has_relationship(
        db_client,
        subscriber.id,
        current_user.id,
        RelationshipType::Follow,
    ).await?;
    let is_subscriber = has_relationship(
        db_client,
        subscriber.id,
        current_user.id,
        RelationshipType::Subscription,
    ).await?;
    if !is_follower && !is_subscriber {
        return Err(ValidationError("account should be either follower or subscriber").into());
    };
    let subscription = create_or_update_local_subscription(
        db_client,
        &subscriber, // sender
        &current_user, // recipient
        subscriber_data.duration.into(),
    ).await?;
    if let Some(ref remote_subscriber) = subscriber.actor_json {
        prepare_add_subscriber(
            &config.instance(),
            remote_subscriber,
            &current_user,
            subscription.expires_at,
            None, // no invoice
        ).save_and_enqueue(db_client).await?;
    };
    let details = SubscriptionDetails::from(subscription);
    Ok(HttpResponse::Ok().json(details))
}

#[get("/options")]
async fn get_subscription_options(
    auth: BearerAuth,
    db_pool: web::Data<DatabaseConnectionPool>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let options: Vec<SubscriptionOption> = current_user.profile
        .payment_options.into_inner().into_iter()
        .filter_map(SubscriptionOption::from_payment_option)
        .collect();
    Ok(HttpResponse::Ok().json(options))
}

#[post("/options")]
async fn register_subscription_option(
    auth: BearerAuth,
    config: web::Data<Config>,
    connection_info: ConnectionInfo,
    db_pool: web::Data<DatabaseConnectionPool>,
    subscription_option: web::Json<SubscriptionOption>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let mut current_user = get_current_user(db_client, auth.token()).await?;
    if !current_user.role.has_permission(Permission::ManageSubscriptionOptions) {
        return Err(MastodonError::PermissionError);
    };

    let payment_option = match subscription_option.into_inner() {
        SubscriptionOption::Monero { chain_id, price, payout_address } => {
            let monero_config = config.monero_config()
                .ok_or(MastodonError::NotSupported)?;
            if chain_id != monero_config.chain_id {
                return Err(ValidationError("unexpected chain ID").into());
            };
            let price: NonZeroU64 = price.try_into()
                .map_err(|_| ValidationError("price must be greater than 0"))?;
            validate_subscription_price(price)?;
            validate_monero_address(&payout_address)
                .map_err(|_| ValidationError("invalid monero address"))?;
            let payment_option = PaymentOption::monero_subscription(
                chain_id,
                price,
                payout_address,
            );
            payment_option
        },
    };
    let mut profile_data = ProfileUpdateData::from(&current_user.profile);
    profile_data.add_payment_option(payment_option);
    // Media cleanup is not needed
    let (updated_profile, _) = update_profile(
        db_client,
        current_user.id,
        profile_data,
    ).await?;
    current_user.profile = updated_profile;

    // Federate
    let media_server = MediaServer::new(&config);
    prepare_update_person(
        db_client,
        &config.instance(),
        &media_server,
        &current_user,
    ).await?.save_and_enqueue(db_client).await?;

    let base_url = get_request_base_url(connection_info);
    let media_server = ClientMediaServer::new(&config, &base_url);
    let account = Account::from_user(
        config.instance().uri_str(),
        &media_server,
        current_user,
    );
    Ok(HttpResponse::Ok().json(account))
}

#[get("/find")]
async fn find_subscription(
    db_pool: web::Data<DatabaseConnectionPool>,
    query_params: web::Query<SubscriptionQueryParams>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let subscription = get_subscription_by_participants(
        db_client,
        query_params.sender_id,
        query_params.recipient_id,
    ).await?;
    let details = SubscriptionDetails::from(subscription);
    Ok(HttpResponse::Ok().json(details))
}

#[post("/invoices")]
async fn create_invoice_view(
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
    invoice_data: web::Json<InvoiceData>,
) -> Result<HttpResponse, MastodonError> {
    if invoice_data.sender_id == invoice_data.recipient_id {
        return Err(ValidationError("sender must be different from recipient").into());
    };
    validate_amount(invoice_data.amount)?;
    let db_client = &**get_database_client(&db_pool).await?;
    let sender = get_profile_by_id(db_client, invoice_data.sender_id).await?;
    let recipient = get_profile_by_id(db_client, invoice_data.recipient_id).await?;

    let db_invoice = if recipient.is_local() {
        // Local recipient
        let monero_config = config.monero_config()
            .ok_or(MastodonError::NotSupported)?;
        if invoice_data.chain_id != monero_config.chain_id {
            return Err(ValidationError("unexpected chain ID").into());
        };
        let _subscription_option: MoneroSubscription = recipient
            .payment_options
            .find_subscription_option(&invoice_data.chain_id)
            .ok_or(ValidationError("recipient can't accept payment"))?;
        let payment_address = create_monero_address(monero_config).await
            .map_err(MastodonError::from_internal)?
            .to_string();
        create_local_invoice(
            db_client,
            sender.id,
            recipient.id,
            &invoice_data.chain_id,
            &payment_address,
            invoice_data.amount,
        ).await?
    } else {
        // Remote recipient; the sender must be local
        let sender = get_user_by_id(db_client, sender.id).await?;
        let recipient_actor = recipient.actor_json.as_ref()
            .expect("actor data should be present");
        let subscription_option: RemoteMoneroSubscription = recipient
            .payment_options
            .find_subscription_option(&invoice_data.chain_id)
            .ok_or(ValidationError("recipient can't accept payment"))?;
        if !subscription_option.fep_0837_enabled {
            return Err(MastodonError::OperationError("recipient doesn't support FEP-0837"));
        };
        let db_invoice = create_remote_invoice(
            db_client,
            sender.id,
            recipient.id,
            &invoice_data.chain_id,
            invoice_data.amount,
        ).await?;
        prepare_offer_agreement(
            &config.instance(),
            &sender,
            recipient_actor,
            &subscription_option,
            db_invoice.id,
            invoice_data.amount,
        ).save_and_enqueue(db_client).await?;
        db_invoice
    };
    let invoice = Invoice::from(db_invoice);
    Ok(HttpResponse::Ok().json(invoice))
}

#[get("/invoices/{invoice_id}")]
async fn get_invoice_view(
    db_pool: web::Data<DatabaseConnectionPool>,
    invoice_id: web::Path<Uuid>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let db_invoice = get_invoice_by_id(db_client, *invoice_id).await?;
    let invoice = Invoice::from(db_invoice);
    Ok(HttpResponse::Ok().json(invoice))
}

#[delete("/invoices/{invoice_id}")]
async fn cancel_invoice_view(
    db_pool: web::Data<DatabaseConnectionPool>,
    invoice_id: web::Path<Uuid>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let db_invoice = set_invoice_status(
        db_client,
        *invoice_id,
        InvoiceStatus::Cancelled,
    ).await?;
    let invoice = Invoice::from(db_invoice);
    Ok(HttpResponse::Ok().json(invoice))
}

pub fn subscription_api_scope() -> Scope {
    web::scope("/v1/subscriptions")
        .service(create_subscription_view)
        .service(get_subscription_options)
        .service(register_subscription_option)
        .service(find_subscription)
        .service(create_invoice_view)
        .service(get_invoice_view)
        .service(cancel_invoice_view)
}
