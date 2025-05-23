/// https://docs.joinmastodon.org/methods/follow_requests/
use actix_web::{
    dev::ConnectionInfo,
    http::Uri,
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
        accept_follow::prepare_accept_follow,
        reject_follow::prepare_reject_follow,
    },
};
use mitra_config::Config;
use mitra_models::{
    database::{
        get_database_client,
        DatabaseConnectionPool,
        DatabaseError,
    },
    profiles::queries::get_profile_by_id,
    relationships::queries::{
        follow_request_accepted,
        follow_request_rejected,
        get_follow_request_by_participants,
        get_follow_requests_paginated,
    },
};

use crate::http::get_request_base_url;
use crate::mastodon_api::{
    accounts::{
        helpers::get_relationship,
        types::Account,
    },
    auth::get_current_user,
    errors::MastodonError,
    media_server::ClientMediaServer,
    pagination::{get_last_item, get_paginated_response},
};

use super::types::RequestListQueryParams;

#[get("")]
async fn follow_request_list(
    auth: BearerAuth,
    config: web::Data<Config>,
    connection_info: ConnectionInfo,
    db_pool: web::Data<DatabaseConnectionPool>,
    request_uri: Uri,
    query_params: web::Query<RequestListQueryParams>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let profiles = get_follow_requests_paginated(
        db_client,
        current_user.id,
        query_params.max_id,
        query_params.limit.inner(),
    ).await?;
    let maybe_last_id = get_last_item(&profiles, &query_params.limit)
        .map(|item| item.related_id);
    let base_url = get_request_base_url(connection_info);
    let media_server = ClientMediaServer::new(&config, &base_url);
    let instance_url = config.instance().url();
    let accounts: Vec<Account> = profiles.into_iter()
        .map(|item| Account::from_profile(
            &instance_url,
            &media_server,
            item.profile,
        ))
        .collect();
    let response = get_paginated_response(
        &base_url,
        &request_uri,
        accounts,
        maybe_last_id,
    );
    Ok(response)
}

#[post("/{account_id}/authorize")]
async fn accept_follow_request_view(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
    account_id: web::Path<Uuid>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let source_profile = get_profile_by_id(db_client, *account_id).await?;
    let follow_request = get_follow_request_by_participants(
        db_client,
        source_profile.id,
        current_user.id,
    ).await?;
    follow_request_accepted(db_client, follow_request.id).await?;
    if let Some(remote_actor) = source_profile.actor_json {
        // Activity ID should be known
        let activity_id = follow_request.activity_id
            .ok_or(DatabaseError::type_error())?;
        prepare_accept_follow(
            &config.instance(),
            &current_user,
            &remote_actor,
            &activity_id,
        )?.save_and_enqueue(db_client).await?;
    };
    let relationship = get_relationship(
        db_client,
        current_user.id,
        source_profile.id,
    ).await?;
    Ok(HttpResponse::Ok().json(relationship))
}

#[post("/{account_id}/reject")]
async fn reject_follow_request_view(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
    account_id: web::Path<Uuid>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let source = get_profile_by_id(db_client, *account_id).await?;
    let follow_request = get_follow_request_by_participants(
        db_client,
        source.id,
        current_user.id,
    ).await?;
    follow_request_rejected(db_client, follow_request.id).await?;
    if let Some(remote_actor) = source.actor_json {
        let activity_id = follow_request.activity_id
            .ok_or(DatabaseError::type_error())?;
        prepare_reject_follow(
            &config.instance(),
            &current_user,
            &remote_actor,
            &activity_id,
        )?.save_and_enqueue(db_client).await?;
    };
    let relationship = get_relationship(
        db_client,
        current_user.id,
        source.id,
    ).await?;
    Ok(HttpResponse::Ok().json(relationship))
}

pub fn follow_request_api_scope() -> Scope {
    web::scope("/v1/follow_requests")
        .service(follow_request_list)
        .service(accept_follow_request_view)
        .service(reject_follow_request_view)
}
