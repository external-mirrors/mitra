use actix_web::{
    dev::ConnectionInfo,
    get,
    post,
    web,
    HttpResponse,
    Scope,
};
use actix_web_httpauth::extractors::bearer::BearerAuth;

use mitra_config::Config;
use mitra_models::{
    database::{
        get_database_client,
        DatabaseConnectionPool,
        DatabaseError,
    },
    profiles::helpers::find_verified_aliases,
    profiles::queries::{
        get_profile_by_acct,
        get_profile_by_remote_actor_id,
        update_profile,
    },
    profiles::types::ProfileUpdateData,
    users::queries::{
        set_user_password,
        update_client_config,
    },
    users::types::ClientConfig,
};
use mitra_utils::passwords::hash_password;
use mitra_validators::{
    errors::ValidationError,
    users::validate_client_config_update,
};

use crate::activitypub::{
    builders::update_person::prepare_update_person,
    identifiers::profile_actor_id,
};
use crate::http::get_request_base_url;
use crate::mastodon_api::{
    accounts::helpers::get_aliases,
    accounts::types::Account,
    errors::MastodonError,
    oauth::auth::get_current_user,
};

use super::helpers::{
    export_followers,
    export_follows,
    import_follows_task,
    move_followers_task,
    parse_address_list,
};
use super::types::{
    AddAliasRequest,
    ImportFollowsRequest,
    MoveFollowersRequest,
    PasswordChangeRequest,
    RemoveAliasRequest,
};

// Similar to Pleroma settings store
// https://docs-develop.pleroma.social/backend/development/API/differences_in_mastoapi_responses/#pleroma-settings-store
#[post("/client_config")]
async fn client_config_view(
    auth: BearerAuth,
    connection_info: ConnectionInfo,
    config: web::Data<Config>,
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
        &current_user.id,
        client_name,
        client_config_value,
    ).await?;
    let account = Account::from_user(
        &get_request_base_url(connection_info),
        &config.instance_url(),
        current_user,
    );
    Ok(HttpResponse::Ok().json(account))
}

#[post("/change_password")]
async fn change_password_view(
    auth: BearerAuth,
    connection_info: ConnectionInfo,
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
    request_data: web::Json<PasswordChangeRequest>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let mut current_user = get_current_user(db_client, auth.token()).await?;
    let password_hash = hash_password(&request_data.new_password)
        .map_err(|_| MastodonError::InternalError)?;
    set_user_password(db_client, &current_user.id, &password_hash).await?;
    current_user.password_hash = Some(password_hash);
    let account = Account::from_user(
        &get_request_base_url(connection_info),
        &config.instance_url(),
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
    let instance = config.instance();
    let alias_id = profile_actor_id(&instance.url(), &alias);
    let mut profile_data = ProfileUpdateData::from(&current_user.profile);
    if !profile_data.aliases.contains(&alias_id) {
        profile_data.aliases.push(alias_id);
    } else {
        return Err(ValidationError("alias already exists").into());
    };
    // Media cleanup is not needed
    let (updated_profile, _) = update_profile(
        db_client,
        &current_user.id,
        profile_data,
    ).await?;
    current_user.profile = updated_profile;
    prepare_update_person(
        db_client,
        &instance,
        &current_user,
    ).await?.enqueue(db_client).await?;
    let aliases = get_aliases(
        db_client,
        &get_request_base_url(connection_info),
        &instance.url(),
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
        return Err(MastodonError::NotFoundError("alias"));
    };
    // Media cleanup is not needed
    let (updated_profile, _) = update_profile(
        db_client,
        &current_user.id,
        profile_data,
    ).await?;
    current_user.profile = updated_profile;
    prepare_update_person(
        db_client,
        &instance,
        &current_user,
    ).await?.enqueue(db_client).await?;
    let aliases = get_aliases(
        db_client,
        &get_request_base_url(connection_info),
        &instance.url(),
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
        &current_user.id,
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
        &current_user.id,
    ).await?;
    let response = HttpResponse::Ok()
        .content_type("text/csv")
        .body(csv);
    Ok(response)
}

#[post("/import_follows")]
async fn import_follows_view(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
    request_data: web::Json<ImportFollowsRequest>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let address_list = parse_address_list(&request_data.follows_csv)?;
    // TODO: use job queue
    tokio::spawn(async move {
        import_follows_task(
            &config,
            current_user,
            &db_pool,
            address_list,
        ).await.unwrap_or_else(|error| {
            log::error!("import follows: {}", error);
        });
    });
    Ok(HttpResponse::NoContent().finish())
}

#[post("/move_followers")]
async fn move_followers(
    auth: BearerAuth,
    connection_info: ConnectionInfo,
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
    request_data: web::Json<MoveFollowersRequest>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    if current_user.profile.identity_proofs.inner().is_empty() {
        return Err(ValidationError("identity proof is required").into());
    };
    let instance = config.instance();
    if request_data.from_actor_id.starts_with(&instance.url()) {
        return Err(ValidationError("can't move from local actor").into());
    };
    // Existence of actor is not verified because
    // the old profile could have been deleted
    let maybe_from_profile = match get_profile_by_remote_actor_id(
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
            .map(|profile| profile_actor_id(&instance.url(), &profile));
        if !aliases.any(|actor_id| actor_id == request_data.from_actor_id) {
            return Err(ValidationError("old profile is not an alias").into());
        };
    };
    let address_list = parse_address_list(&request_data.followers_csv)?;
    let current_user_clone = current_user.clone();
    // TODO: use job queue
    tokio::spawn(async move {
        move_followers_task(
            &config,
            &db_pool,
            current_user_clone,
            &request_data.from_actor_id,
            maybe_from_profile,
            address_list,
        ).await.unwrap_or_else(|error| {
            log::error!("move followers: {}", error);
        });
    });

    let account = Account::from_user(
        &get_request_base_url(connection_info),
        &instance.url(),
        current_user,
    );
    Ok(HttpResponse::Ok().json(account))
}

pub fn settings_api_scope() -> Scope {
    web::scope("/api/v1/settings")
        .service(client_config_view)
        .service(change_password_view)
        .service(add_alias_view)
        .service(remove_alias_view)
        .service(export_followers_view)
        .service(export_follows_view)
        .service(import_follows_view)
        .service(move_followers)
}
