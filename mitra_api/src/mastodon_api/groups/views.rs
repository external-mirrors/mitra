use actix_web::{
    delete,
    dev::ConnectionInfo,
    get,
    post,
    patch,
    web,
    HttpResponse,
    Scope,
};
use actix_web_httpauth::extractors::bearer::BearerAuth;
use apx_sdk::core::crypto::{
    eddsa::generate_ed25519_key,
    rsa::generate_rsa_key,
};
use uuid::Uuid;

use mitra_activitypub::{
    adapters::{
        follow_requests::accept_and_add_follower,
        users::{
            create_or_update_local_actor,
            delete_account,
        },
    },
    authority::Authority,
    builders::update_person::prepare_update_person,
};
use mitra_config::Config;
use mitra_models::{
    accounts::queries::get_group_account_by_id,
    database::{
        get_database_client,
        DatabaseConnectionPool,
    },
    groups::{
        queries::{
            create_group,
            get_related_groups,
        },
        types::GroupCreateData,
    },
    posts::helpers::can_create_post,
    profiles::{
        queries::update_profile,
        types::ProfileUpdateData,
    },
    relationships::{
        helpers::create_follow_request,
        queries::has_relationship,
        types::RelationshipType,
    },
};
use mitra_services::media::MediaServer;
use mitra_validators::{
    groups::{
        clean_group_create_data,
        validate_group_create_data,
    },
    profiles::clean_profile_update_data,
};

use crate::{
    http::{
        get_request_base_url,
    },
    mastodon_api::{
        accounts::{
            helpers::{
                parse_microsyntaxes,
                parse_profile_bio,
            },
            types::Account,
        },
        auth::get_current_user,
        errors::MastodonError,
        media_server::ClientMediaServer,
    },
};

use super::types::{
    GroupCreateForm,
    GroupListQueryParams,
    GroupSource,
    GroupUpdateForm,
};

#[post("")]
async fn create_group_view(
    auth: BearerAuth,
    config: web::Data<Config>,
    connection_info: ConnectionInfo,
    db_pool: web::Data<DatabaseConnectionPool>,
    group_form: web::Json<GroupCreateForm>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    if !can_create_post(&current_user) {
        return Err(MastodonError::PermissionError);
    };
    let maybe_bio_source = group_form.description.clone();
    let maybe_bio_html = parse_profile_bio(maybe_bio_source.as_ref())?;
    let profile_text = parse_microsyntaxes(
        db_client,
        None,
        maybe_bio_html.as_ref(),
    ).await?;
    let rsa_secret_key = match web::block(generate_rsa_key).await {
        Ok(Ok(secret_key)) => secret_key,
        Ok(Err(error)) => return Err(MastodonError::from_internal(error)),
        Err(error) => return Err(MastodonError::from_internal(error)),
    };
    let ed25519_secret_key = generate_ed25519_key();
    let mut group_data = GroupCreateData {
        username: group_form.name.clone(),
        bio: profile_text.bio,
        bio_source: maybe_bio_source,
        emojis: profile_text.emojis,
        rsa_secret_key,
        ed25519_secret_key,
    };
    clean_group_create_data(&mut group_data);
    validate_group_create_data(&group_data)?;
    let group = create_group(
        db_client,
        current_user.id,
        group_data,
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

#[get("/{group_id}/source")]
async fn get_group_source(
    auth: BearerAuth,
    db_pool: web::Data<DatabaseConnectionPool>,
    group_id: web::Path<Uuid>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let group = get_group_account_by_id(db_client, *group_id).await?;
    if !has_relationship(
        db_client,
        current_user.id,
        group.id,
        RelationshipType::GroupAdmin,
    ).await? {
        return Err(MastodonError::PermissionError);
    };
    let source = GroupSource {
        description: group.profile.bio_source,
    };
    Ok(HttpResponse::Ok().json(source))
}

#[patch("/{group_id}")]
async fn update_group_view(
    auth: BearerAuth,
    config: web::Data<Config>,
    connection_info: ConnectionInfo,
    db_pool: web::Data<DatabaseConnectionPool>,
    group_id: web::Path<Uuid>,
    group_form: web::Json<GroupUpdateForm>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let mut group = get_group_account_by_id(db_client, *group_id).await?;
    if !has_relationship(
        db_client,
        current_user.id,
        group.id,
        RelationshipType::GroupAdmin,
    ).await? {
        return Err(MastodonError::PermissionError);
    };
    let mut group_data = ProfileUpdateData::from(&group.profile);
    // Partial update
    if let Some(ref bio_source) = group_form.description {
        let maybe_bio_html = parse_profile_bio(Some(bio_source))?;
        let profile_text = parse_microsyntaxes(
            db_client,
            None,
            maybe_bio_html.as_ref(),
        ).await?;
        group_data.bio = profile_text.bio;
        group_data.bio_source = Some(bio_source.clone());
        group_data.emojis = profile_text.emojis;
    };
    clean_profile_update_data(&mut group_data)?;
    let (updated_profile, _) = update_profile(
        db_client,
        group.id,
        group_data,
    ).await?;
    group.profile = updated_profile;
    create_or_update_local_actor(&config, db_client, &group).await?;

    let media_server = MediaServer::new(&config);
    let instance = config.instance();
    prepare_update_person(
        db_client,
        &instance,
        &media_server,
        &group,
    ).await?.save_and_enqueue(db_client).await?;

    let base_url = get_request_base_url(connection_info);
    let authority = Authority::from(&instance);
    let media_server = ClientMediaServer::new(&config, &base_url);
    let account = Account::from_profile(
        &authority,
        &media_server,
        group.profile,
    );
    Ok(HttpResponse::Ok().json(account))
}

#[delete("/{group_id}")]
async fn delete_group_view(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
    group_id: web::Path<Uuid>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let group = get_group_account_by_id(db_client, *group_id).await?;
    if !has_relationship(
        db_client,
        current_user.id,
        group.id,
        RelationshipType::GroupAdmin,
    ).await? {
        return Err(MastodonError::PermissionError);
    };
    delete_account(&config, db_client, &group).await?;
    let empty = serde_json::json!({});
    Ok(HttpResponse::NoContent().json(empty))
}

pub fn group_api_scope() -> Scope {
    web::scope("/v1/groups")
        .service(create_group_view)
        .service(get_followed_groups_view)
        .service(get_group_source)
        .service(update_group_view)
        .service(delete_group_view)
}
