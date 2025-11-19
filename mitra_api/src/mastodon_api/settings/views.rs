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
use chrono::Utc;

use mitra_activitypub::{
    adapters::users::delete_user,
    builders::{
        follow::follow_or_create_request,
        move_person::prepare_move_person,
        update_person::prepare_update_person,
    },
    identifiers::profile_actor_id,
};
use mitra_config::Config;
use mitra_models::{
    database::{
        get_database_client,
        DatabaseConnectionPool,
        DatabaseError,
    },
    notifications::helpers::create_move_notification,
    oauth::queries::{
        delete_oauth_token_by_id,
        get_oauth_tokens,
    },
    profiles::helpers::find_verified_aliases,
    profiles::queries::{
        get_profile_by_acct,
        get_remote_profile_by_actor_id,
        update_profile,
    },
    profiles::types::ProfileUpdateData,
    relationships::queries::{get_followers, unfollow},
    users::queries::{
        get_user_by_id,
        set_user_password,
        update_client_config,
    },
    users::types::ClientConfig,
};
use mitra_services::media::MediaServer;
use mitra_utils::passwords::hash_password;
use mitra_validators::{
    errors::ValidationError,
    profiles::validate_aliases,
    users::validate_client_config_update,
};
use mitra_workers::importer::ImporterJobData;

use crate::http::get_request_base_url;
use crate::mastodon_api::{
    accounts::helpers::get_aliases,
    accounts::types::Account,
    auth::{get_current_session, get_current_user},
    errors::MastodonError,
    media_server::ClientMediaServer,
};

use super::helpers::{
    export_followers,
    export_follows,
    parse_address_list,
};
use super::types::{
    AddAliasRequest,
    ImportFollowersRequest,
    ImportFollowsRequest,
    MoveFollowersRequest,
    PasswordChangeRequest,
    RemoveAliasRequest,
    Session,
};

// Similar to Pleroma settings store
// https://docs-develop.pleroma.social/backend/development/API/differences_in_mastoapi_responses/#pleroma-settings-store
#[post("/client_config")]
async fn client_config_view(
    auth: BearerAuth,
    config: web::Data<Config>,
    connection_info: ConnectionInfo,
    db_pool: web::Data<DatabaseConnectionPool>,
    request_data: web::Json<ClientConfig>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let mut current_user = get_current_user(db_client, auth.token()).await?;
    validate_client_config_update(
        &current_user.client_config,
        &request_data,
    )?;
    let (client_name, client_config_value) =
        request_data.iter().next().expect("hashmap entry should exist");
    current_user.client_config = update_client_config(
        db_client,
        current_user.id,
        client_name,
        client_config_value,
    ).await?;
    let base_url = get_request_base_url(connection_info);
    let media_server = ClientMediaServer::new(&config, &base_url);
    let account = Account::from_user(
        config.instance().uri_str(),
        &media_server,
        current_user,
    );
    Ok(HttpResponse::Ok().json(account))
}

#[get("/sessions")]
async fn session_list_view(
    auth: BearerAuth,
    db_pool: web::Data<DatabaseConnectionPool>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let (current_session_id, current_user) =
        get_current_session(db_client, auth.token()).await?;
    let tokens = get_oauth_tokens(db_client, current_user.id).await?;
    let sessions: Vec<_> = tokens.into_iter()
        .filter(|token| token.expires_at >= Utc::now())
        .map(Session::from_db)
        .map(|mut session| {
            if session.id == current_session_id {
                session.is_current = true;
            };
            session
        })
        .collect();
    Ok(HttpResponse::Ok().json(sessions))
}

#[delete("/sessions/{session_id}")]
async fn terminate_session_view(
    auth: BearerAuth,
    db_pool: web::Data<DatabaseConnectionPool>,
    session_id: web::Path<i32>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    delete_oauth_token_by_id(
        db_client,
        current_user.id,
        session_id.into_inner(),
    ).await?;
    Ok(HttpResponse::NoContent().finish())
}

#[post("/change_password")]
async fn change_password_view(
    auth: BearerAuth,
    config: web::Data<Config>,
    connection_info: ConnectionInfo,
    db_pool: web::Data<DatabaseConnectionPool>,
    request_data: web::Json<PasswordChangeRequest>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let mut current_user = get_current_user(db_client, auth.token()).await?;
    let password_digest = hash_password(&request_data.new_password)
        .map_err(MastodonError::from_internal)?;
    set_user_password(db_client, current_user.id, &password_digest).await?;
    current_user.password_digest = Some(password_digest);
    let base_url = get_request_base_url(connection_info);
    let media_server = ClientMediaServer::new(&config, &base_url);
    let account = Account::from_user(
        config.instance().uri_str(),
        &media_server,
        current_user,
    );
    Ok(HttpResponse::Ok().json(account))
}

#[post("/aliases")]
async fn add_alias_view(
    auth: BearerAuth,
    config: web::Data<Config>,
    connection_info: ConnectionInfo,
    db_pool: web::Data<DatabaseConnectionPool>,
    request_data: web::Json<AddAliasRequest>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let mut current_user = get_current_user(db_client, auth.token()).await?;
    let alias = get_profile_by_acct(db_client, &request_data.acct).await?;
    if alias.id == current_user.id {
        return Err(ValidationError("alias must differ from current account").into());
    };
    if alias.is_local() {
        return Err(ValidationError("alias must be on another server").into());
    };
    let instance = config.instance();
    let alias_id = profile_actor_id(instance.uri_str(), &alias);
    let mut profile_data = ProfileUpdateData::from(&current_user.profile);
    if !profile_data.aliases.contains(&alias_id) {
        profile_data.aliases.push(alias_id);
    } else {
        return Err(ValidationError("alias already exists").into());
    };
    validate_aliases(&profile_data.aliases)?;
    // Media cleanup is not needed
    let (updated_profile, _) = update_profile(
        db_client,
        current_user.id,
        profile_data,
    ).await?;
    current_user.profile = updated_profile;
    let media_server = MediaServer::new(&config);
    prepare_update_person(
        db_client,
        &instance,
        &media_server,
        &current_user,
    ).await?.save_and_enqueue(db_client).await?;
    let base_url = get_request_base_url(connection_info);
    let media_server = ClientMediaServer::new(&config, &base_url);
    let aliases = get_aliases(
        db_client,
        instance.uri_str(),
        &media_server,
        &current_user.profile,
    ).await?;
    Ok(HttpResponse::Ok().json(aliases))
}

#[post("/aliases/remove")]
async fn remove_alias_view(
    auth: BearerAuth,
    config: web::Data<Config>,
    connection_info: ConnectionInfo,
    db_pool: web::Data<DatabaseConnectionPool>,
    request_data: web::Json<RemoveAliasRequest>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let mut current_user = get_current_user(db_client, auth.token()).await?;
    let instance = config.instance();
    let mut profile_data = ProfileUpdateData::from(&current_user.profile);
    if profile_data.aliases.contains(&request_data.actor_id) {
        profile_data.aliases.retain(|alias| alias != &request_data.actor_id);
    } else {
        return Err(MastodonError::NotFound("alias"));
    };
    // Media cleanup is not needed
    let (updated_profile, _) = update_profile(
        db_client,
        current_user.id,
        profile_data,
    ).await?;
    current_user.profile = updated_profile;
    let media_server = MediaServer::new(&config);
    prepare_update_person(
        db_client,
        &instance,
        &media_server,
        &current_user,
    ).await?.save_and_enqueue(db_client).await?;
    let base_url = get_request_base_url(connection_info);
    let media_server = ClientMediaServer::new(&config, &base_url);
    let aliases = get_aliases(
        db_client,
        instance.uri_str(),
        &media_server,
        &current_user.profile,
    ).await?;
    Ok(HttpResponse::Ok().json(aliases))
}

#[get("/export_followers")]
async fn export_followers_view(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let csv = export_followers(
        db_client,
        &config.instance().hostname(),
        current_user.id,
    ).await?;
    let response = HttpResponse::Ok()
        .content_type("text/csv")
        .body(csv);
    Ok(response)
}

#[get("/export_follows")]
async fn export_follows_view(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let csv = export_follows(
        db_client,
        &config.instance().hostname(),
        current_user.id,
    ).await?;
    let response = HttpResponse::Ok()
        .content_type("text/csv")
        .body(csv);
    Ok(response)
}

#[post("/import_follows")]
async fn import_follows_view(
    auth: BearerAuth,
    db_pool: web::Data<DatabaseConnectionPool>,
    request_data: web::Json<ImportFollowsRequest>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let address_list = parse_address_list(&request_data.follows_csv)?
        .iter()
        .map(|address| address.to_string())
        .collect();
    let job_data = ImporterJobData::Follows {
        user_id: current_user.id,
        address_list: address_list,
    };
    job_data.into_job(db_client).await?;
    Ok(HttpResponse::NoContent().finish())
}

#[post("/import_followers")]
async fn import_followers_view(
    auth: BearerAuth,
    config: web::Data<Config>,
    connection_info: ConnectionInfo,
    db_pool: web::Data<DatabaseConnectionPool>,
    request_data: web::Json<ImportFollowersRequest>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    if current_user.profile.identity_proofs.inner().is_empty() {
        return Err(ValidationError("identity proof is required").into());
    };
    let instance = config.instance();
    if request_data.from_actor_id.starts_with(instance.uri_str()) {
        return Err(ValidationError("can't move from local actor").into());
    };
    // Existence of actor is not verified because
    // the old profile could have been deleted
    let maybe_from_profile = match get_remote_profile_by_actor_id(
        db_client,
        &request_data.from_actor_id,
    ).await {
        Ok(profile) => Some(profile),
        Err(DatabaseError::NotFound(_)) => None,
        Err(other_error) => return Err(other_error.into()),
    };
    if maybe_from_profile.is_some() {
        // Find known aliases of the current user
        let mut aliases = find_verified_aliases(
            db_client,
            &current_user.profile,
        ).await?
            .into_iter()
            .map(|profile| profile_actor_id(instance.uri_str(), &profile));
        if !aliases.any(|actor_id| actor_id == request_data.from_actor_id) {
            return Err(ValidationError("old profile is not an alias").into());
        };
    };
    let address_list = parse_address_list(&request_data.followers_csv)?
        .iter()
        .map(|address| address.to_string())
        .collect();
    let job_data = ImporterJobData::Followers {
        user_id: current_user.id,
        from_actor_id: request_data.from_actor_id.clone(),
        address_list,
    };
    job_data.into_job(db_client).await?;

    let base_url = get_request_base_url(connection_info);
    let media_server = ClientMediaServer::new(&config, &base_url);
    let account = Account::from_user(
        instance.uri_str(),
        &media_server,
        current_user,
    );
    Ok(HttpResponse::Ok().json(account))
}

#[post("/move_followers")]
async fn move_followers_view(
    auth: BearerAuth,
    config: web::Data<Config>,
    connection_info: ConnectionInfo,
    db_pool: web::Data<DatabaseConnectionPool>,
    request_data: web::Json<MoveFollowersRequest>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let instance = config.instance();
    let current_user = get_current_user(db_client, auth.token()).await?;
    let current_actor_id = profile_actor_id(
        instance.uri_str(),
        &current_user.profile,
    );
    let target = get_profile_by_acct(db_client, &request_data.target_acct).await?;
    if !target.aliases.contains(&current_actor_id) {
        return Err(ValidationError("target is not an alias").into());
    };
    if target.is_local() {
        return Err(ValidationError("can't move followers to a local actor").into());
    };
    let followers = get_followers(db_client, current_user.id).await?;
    let mut remote_followers = vec![];
    for follower in followers {
        if follower.id == target.id {
            continue;
        };
        if let Some(remote_actor) = follower.actor_json {
            remote_followers.push(remote_actor);
            continue;
        };
        let follower = get_user_by_id(db_client, follower.id).await?;
        unfollow(db_client, follower.id, current_user.id).await?;
        follow_or_create_request(
            db_client,
            &instance,
            &follower,
            &target,
        ).await?;
        create_move_notification(
            db_client,
            target.id,
            follower.id,
        ).await?;
    };
    let target_actor_id = profile_actor_id(instance.uri_str(), &target);
    prepare_move_person(
        &instance,
        &current_user,
        &target_actor_id,
        false, // push mode
        remote_followers,
    ).save_and_enqueue(db_client).await?;

    let base_url = get_request_base_url(connection_info);
    let media_server = ClientMediaServer::new(&config, &base_url);
    let account = Account::from_user(
        instance.uri_str(),
        &media_server,
        current_user,
    );
    Ok(HttpResponse::Ok().json(account))
}

#[post("/delete_account")]
async fn delete_account_view(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    delete_user(&config, db_client, &current_user).await?;
    Ok(HttpResponse::NoContent().finish())
}

pub fn settings_api_scope() -> Scope {
    web::scope("/v1/settings")
        .service(client_config_view)
        .service(session_list_view)
        .service(terminate_session_view)
        .service(change_password_view)
        .service(add_alias_view)
        .service(remove_alias_view)
        .service(export_followers_view)
        .service(export_follows_view)
        .service(import_follows_view)
        .service(import_followers_view)
        .service(move_followers_view)
        .service(delete_account_view)
}
