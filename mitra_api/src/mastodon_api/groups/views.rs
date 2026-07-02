use actix_web::{
    dev::ConnectionInfo,
    get,
    post,
    web,
    HttpResponse,
    Scope,
};
use actix_web_httpauth::extractors::bearer::BearerAuth;
use apx_sdk::core::crypto::{
    eddsa::generate_ed25519_key,
    rsa::generate_rsa_key,
};

use mitra_activitypub::{
    adapters::{
        follow_requests::accept_and_add_follower,
        users::create_or_update_local_actor,
    },
    authority::Authority,
};
use mitra_config::Config;
use mitra_models::{
    database::{
        get_database_client,
        DatabaseConnectionPool,
    },
    groups::queries::{
        create_group,
        get_related_groups,
    },
    posts::helpers::can_create_post,
    relationships::helpers::create_follow_request,
};
use mitra_validators::accounts::validate_local_username;

use crate::{
    http::{
        get_request_base_url,
    },
    mastodon_api::{
        accounts::types::Account,
        auth::get_current_user,
        errors::MastodonError,
        media_server::ClientMediaServer,
    },
};

use super::types::{
    GroupCreateData,
    GroupListQueryParams,
};

#[post("")]
async fn create_group_view(
    auth: BearerAuth,
    config: web::Data<Config>,
    connection_info: ConnectionInfo,
    db_pool: web::Data<DatabaseConnectionPool>,
    group_data: web::Json<GroupCreateData>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    if !can_create_post(&current_user) {
        return Err(MastodonError::PermissionError);
    };
    let rsa_secret_key = match web::block(generate_rsa_key).await {
        Ok(Ok(secret_key)) => secret_key,
        Ok(Err(error)) => return Err(MastodonError::from_internal(error)),
        Err(error) => return Err(MastodonError::from_internal(error)),
    };
    let ed25519_secret_key = generate_ed25519_key();
    validate_local_username(&group_data.name)?;
    let group = create_group(
        db_client,
        current_user.id,
        group_data.name.clone(),
        rsa_secret_key,
        ed25519_secret_key,
    ).await?;
    create_or_update_local_actor(&config, db_client, &group).await?;
    let follow_request = create_follow_request(
        db_client,
        current_user.id,
        group.id,
    ).await?;
    let authority = Authority::from(&config.instance());
    accept_and_add_follower(
        authority.root(),
        db_client,
        follow_request.id,
    ).await?;
    let base_url = get_request_base_url(connection_info);
    let media_server = ClientMediaServer::new(&config, &base_url);
    let account = Account::from_profile(
        &authority,
        &media_server,
        group.profile,
    );
    Ok(HttpResponse::Ok().json(account))
}

// TODO: use /api/v1/groups
#[get("/followed")]
async fn get_followed_groups_view(
    auth: BearerAuth,
    config: web::Data<Config>,
    connection_info: ConnectionInfo,
    db_pool: web::Data<DatabaseConnectionPool>,
    query_params: web::Query<GroupListQueryParams>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let groups = get_related_groups(
        db_client,
        current_user.id,
        query_params.filter()?,
        query_params.offset,
        query_params.limit.inner(),
    ).await?;
    let base_url = get_request_base_url(connection_info);
    let authority = Authority::from(&config.instance());
    let media_server = ClientMediaServer::new(&config, &base_url);
    let accounts: Vec<Account> = groups.into_iter()
        .map(|profile| Account::from_profile(
            &authority,
            &media_server,
            profile,
        ))
        .collect();
    Ok(HttpResponse::Ok().json(accounts))
}

pub fn group_api_scope() -> Scope {
    web::scope("/v1/groups")
        .service(create_group_view)
        .service(get_followed_groups_view)
}
