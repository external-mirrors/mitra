use actix_web::{
    delete,
    dev::ConnectionInfo,
    get,
    post,
    put,
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
        update_custom_feed,
    },
};
use mitra_validators::custom_feeds::{
    clean_custom_feed_name,
    validate_custom_feed_name,
};

use crate::{
    http::{
        get_request_base_url,
        JsonOrForm,
        MultiQuery,
    },
    mastodon_api::{
        accounts::types::Account,
        auth::get_current_user,
        errors::MastodonError,
        media_server::ClientMediaServer,
        pagination::PageSize,
    },
};

use super::types::{
    List,
    ListAccountsData,
    ListAccountsQueryParams,
    ListData,
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
    list_data: JsonOrForm<ListData>,
) -> Result<HttpResponse, MastodonError> {
    let list_data = list_data.into_inner();
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let feed_name = clean_custom_feed_name(&list_data.title);
    validate_custom_feed_name(feed_name)?;
    let feed = create_custom_feed(
        db_client,
        current_user.id,
        feed_name,
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

/// https://docs.joinmastodon.org/methods/lists/#update
#[put("/{list_id}")]
async fn update_list(
    auth: BearerAuth,
    db_pool: web::Data<DatabaseConnectionPool>,
    list_id: web::Path<i32>,
    list_data: web::Json<ListData>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let feed_name = clean_custom_feed_name(&list_data.title);
    validate_custom_feed_name(feed_name)?;
    let feed = update_custom_feed(
        db_client,
        *list_id,
        current_user.id,
        feed_name,
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
    query_params: web::Query<ListAccountsQueryParams>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let feed = get_custom_feed(
        db_client,
        *list_id,
        current_user.id,
    ).await?;
    let limit = if query_params.limit.inner() == 0 {
        PageSize::MAX
    } else {
        query_params.limit.inner()
    };
    let sources = get_custom_feed_sources(
        db_client,
        feed.id,
        query_params.max_id,
        limit,
    ).await?;
    let base_url = get_request_base_url(connection_info);
    let media_server = ClientMediaServer::new(&config, &base_url);
    let instance = config.instance();
    let accounts: Vec<Account> = sources.into_iter()
        .map(|item| Account::from_profile(
            instance.uri_str(),
            &media_server,
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
        .service(update_list)
        .service(delete_list)
        .service(get_list_accounts)
        .service(add_accounts_to_list)
        .service(remove_accounts_from_list)
}
