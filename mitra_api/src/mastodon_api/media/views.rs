/// https://docs.joinmastodon.org/methods/media/
use actix_multipart::form::MultipartForm;
use actix_web::{
    dev::ConnectionInfo,
    web,
    Either,
    HttpResponse,
    Scope,
};
use actix_web_httpauth::extractors::bearer::BearerAuth;
use uuid::Uuid;

use mitra_config::Config;
use mitra_models::{
    attachments::queries::{
        create_attachment,
        get_attachment,
        update_attachment,
    },
    database::{get_database_client, DatabaseConnectionPool},
    media::types::MediaInfo,
};
use mitra_services::media::MediaStorage;
use mitra_validators::media::validate_media_description;

use crate::{
    http::get_request_base_url,
    mastodon_api::{
        auth::get_current_user,
        errors::MastodonError,
        media_server::ClientMediaServer,
        uploads::save_b64_file,
    },
};

use super::types::{
    Attachment,
    AttachmentForm,
    AttachmentMultipartForm,
    AttachmentUpdateForm,
};

async fn create_attachment_view(
    auth: BearerAuth,
    config: web::Data<Config>,
    connection_info: ConnectionInfo,
    db_pool: web::Data<DatabaseConnectionPool>,
    attachment_form: Either<
        MultipartForm<AttachmentMultipartForm>,
        web::Json<AttachmentForm>,
    >,
) -> Result<HttpResponse, MastodonError> {
    let attachment_form = match attachment_form {
        Either::Left(form) => form.into_inner().into(),
        Either::Right(json) => json.into_inner(),
    };
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let media_storage = MediaStorage::new(&config);
    let file_info = save_b64_file(
        &attachment_form.file,
        &attachment_form.media_type,
        &media_storage,
        config.limits.media.file_size_limit,
        &config.limits.media.supported_media_types(),
    )?;
    if let Some(ref description) = attachment_form.description {
        validate_media_description(description)?;
    };
    let db_attachment = create_attachment(
        db_client,
        current_user.id,
        MediaInfo::local(file_info),
        attachment_form.description.as_deref(),
    ).await?;

    let base_url = get_request_base_url(connection_info);
    let media_server = ClientMediaServer::new(&config, &base_url);
    let attachment = Attachment::from_db(
        &media_server,
        db_attachment,
    );
    Ok(HttpResponse::Ok().json(attachment))
}

async fn get_attachment_view(
    auth: BearerAuth,
    config: web::Data<Config>,
    connection_info: ConnectionInfo,
    db_pool: web::Data<DatabaseConnectionPool>,
    attachment_id: web::Path<Uuid>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let db_attachment = get_attachment(
        db_client,
        current_user.id,
        *attachment_id,
    ).await?;
    let base_url = get_request_base_url(connection_info);
    let media_server = ClientMediaServer::new(&config, &base_url);
    let attachment = Attachment::from_db(
        &media_server,
        db_attachment,
    );
    Ok(HttpResponse::Ok().json(attachment))
}

async fn update_attachment_view(
    auth: BearerAuth,
    config: web::Data<Config>,
    connection_info: ConnectionInfo,
    db_pool: web::Data<DatabaseConnectionPool>,
    attachment_id: web::Path<Uuid>,
    attachment_form: web::Json<AttachmentUpdateForm>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    if let Some(ref description) = attachment_form.description {
        validate_media_description(description)?;
    };
    let db_attachment = update_attachment(
        db_client,
        current_user.id,
        *attachment_id,
        attachment_form.description.as_deref(),
    ).await?;
    let base_url = get_request_base_url(connection_info);
    let media_server = ClientMediaServer::new(&config, &base_url);
    let attachment = Attachment::from_db(
        &media_server,
        db_attachment,
    );
    Ok(HttpResponse::Ok().json(attachment))
}

pub fn media_api_v1_scope() -> Scope {
    web::scope("/v1/media")
        .route("", web::post().to(create_attachment_view))
        .route("/{attachment_id}", web::get().to(get_attachment_view))
        .route("/{attachment_id}", web::put().to(update_attachment_view))
}

pub fn media_api_v2_scope() -> Scope {
    web::scope("/v2/media")
        .route("", web::post().to(create_attachment_view))
}
