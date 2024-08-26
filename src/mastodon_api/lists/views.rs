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
use serde_json;

use mitra_config::Config;
use mitra_models::{
    database::{
        get_database_client,
        DatabaseConnectionPool,
        DatabaseError,
    },
    custom_feeds::queries::{
        add_custom_feed_sources,
        create_custom_feed,
        delete_custom_feed,
        get_custom_feed,
        get_custom_feeds,
        get_custom_feed_sources,
        remove_custom_feed_sources,
    },
};
use mitra_validators::custom_feeds::validate_custom_feed_name;

use crate::{
    http::{get_request_base_url, MultiQuery},
    mastodon_api::{
        accounts::types::Account,
        auth::get_current_user,
        errors::MastodonError,
    },
};

use super::types::{
    AccountListQueryParams,
    List,
    ListAccountsData,
    ListCreateData,
};

/// https://docs.joinmastodon.org/methods/lists/#get
#[get("")]
async fn get_lists(
    auth: BearerAuth,
    db_pool: web::Data<DatabaseConnectionPool>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let feeds = get_custom_feeds(db_client, current_user.id).await?;
    let lists: Vec<List> = feeds.into_iter().map(List::from_db).collect();
    Ok(HttpResponse::Ok().json(lists))
}

/// https://docs.joinmastodon.org/methods/lists/#create
#[post("")]
async fn create_list(
    auth: BearerAuth,
    db_pool: web::Data<DatabaseConnectionPool>,
    list_data: web::Json<ListCreateData>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    validate_custom_feed_name(&list_data.title)?;
    let feed = create_custom_feed(
        db_client,
        current_user.id,
        &list_data.title,
    ).await?;
    let list = List::from_db(feed);
    Ok(HttpResponse::Ok().json(list))
}

/// https://docs.joinmastodon.org/methods/lists/#get-one
#[get("/{list_id}")]
async fn get_list(
    auth: BearerAuth,
    db_pool: web::Data<DatabaseConnectionPool>,
    list_id: web::Path<i32>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let feed = get_custom_feed(
        db_client,
        *list_id,
        current_user.id,
    ).await?;
    let list = List::from_db(feed);
    Ok(HttpResponse::Ok().json(list))
}

/// https://docs.joinmastodon.org/methods/lists/#delete
#[delete("/{list_id}")]
async fn delete_list(
    auth: BearerAuth,
    db_pool: web::Data<DatabaseConnectionPool>,
    list_id: web::Path<i32>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    delete_custom_feed(
        db_client,
        *list_id,
        current_user.id,
    ).await?;
    let empty = serde_json::json!({});
    Ok(HttpResponse::Ok().json(empty))
}

/// https://docs.joinmastodon.org/methods/lists/#accounts
#[get("/{list_id}/accounts")]
async fn get_list_accounts(
    auth: BearerAuth,
    config: web::Data<Config>,
    connection_info: ConnectionInfo,
    db_pool: web::Data<DatabaseConnectionPool>,
    list_id: web::Path<i32>,
    query_params: web::Query<AccountListQueryParams>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let feed = get_custom_feed(
        db_client,
        *list_id,
        current_user.id,
    ).await?;
    let sources = get_custom_feed_sources(
        db_client,
        feed.id,
        query_params.max_id,
        query_params.limit.inner(),
    ).await?;
    let base_url = get_request_base_url(connection_info);
    let instance_url = config.instance().url();
    let accounts: Vec<Account> = sources.into_iter()
        .map(|item| Account::from_profile(
            &base_url,
            &instance_url,
            item,
        ))
        .collect();
    Ok(HttpResponse::Ok().json(accounts))
}

/// https://docs.joinmastodon.org/methods/lists/#accounts-add
#[post("/{list_id}/accounts")]
async fn add_accounts_to_list(
    auth: BearerAuth,
    db_pool: web::Data<DatabaseConnectionPool>,
    list_id: web::Path<i32>,
    accounts_data: web::Json<ListAccountsData>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let feed = get_custom_feed(
        db_client,
        *list_id,
        current_user.id,
    ).await?;
    match add_custom_feed_sources(
        db_client,
        feed.id,
        &accounts_data.account_ids,
    ).await {
        Ok(_) => (),
        Err(DatabaseError::AlreadyExists(_)) => {
            return Err(MastodonError::OperationError("user already added"));
        },
        Err(other_error) => return Err(other_error.into()),
    };
    let empty = serde_json::json!({});
    Ok(HttpResponse::Ok().json(empty))
}

/// https://docs.joinmastodon.org/methods/lists/#accounts-remove
#[delete("/{list_id}/accounts")]
async fn remove_accounts_from_list(
    auth: BearerAuth,
    db_pool: web::Data<DatabaseConnectionPool>,
    list_id: web::Path<i32>,
    query_params: MultiQuery<ListAccountsData>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let feed = get_custom_feed(
        db_client,
        *list_id,
        current_user.id,
    ).await?;
    remove_custom_feed_sources(
        db_client,
        feed.id,
        &query_params.account_ids,
    ).await?;
    let empty = serde_json::json!({});
    Ok(HttpResponse::Ok().json(empty))
}

pub fn list_api_scope() -> Scope {
    web::scope("/v1/lists")
        .service(get_lists)
        .service(create_list)
        .service(get_list)
        .service(delete_list)
        .service(get_list_accounts)
        .service(add_accounts_to_list)
        .service(remove_accounts_from_list)
}
