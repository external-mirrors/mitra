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
        uploads::save_b64_file,
    },
};

use super::types::{
    Attachment,
    AttachmentData,
    AttachmentDataMultipartForm,
    AttachmentUpdateData,
};

async fn create_attachment_view(
    auth: BearerAuth,
    config: web::Data<Config>,
    connection_info: ConnectionInfo,
    db_pool: web::Data<DatabaseConnectionPool>,
    attachment_data: Either<
        MultipartForm<AttachmentDataMultipartForm>,
        web::Json<AttachmentData>,
    >,
) -> Result<HttpResponse, MastodonError> {
    let attachment_data = match attachment_data {
        Either::Left(form) => form.into_inner().into(),
        Either::Right(data) => data.into_inner(),
    };
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let media_storage = MediaStorage::from(config.as_ref());
    let file_info = save_b64_file(
        &attachment_data.file,
        &attachment_data.media_type,
        &media_storage,
        media_storage.file_size_limit,
        &media_storage.supported_media_types(),
    )?;
    if let Some(ref description) = attachment_data.description {
        validate_media_description(description)?;
    };
    let db_attachment = create_attachment(
        db_client,
        current_user.id,
        MediaInfo::local(file_info),
        attachment_data.description.as_deref(),
    ).await?;

    let base_url = get_request_base_url(connection_info);
    let attachment = Attachment::from_db(
        &base_url,
        db_attachment,
    );
    Ok(HttpResponse::Ok().json(attachment))
}

async fn get_attachment_view(
    auth: BearerAuth,
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
    let attachment = Attachment::from_db(
        &base_url,
        db_attachment,
    );
    Ok(HttpResponse::Ok().json(attachment))
}

async fn update_attachment_view(
    auth: BearerAuth,
    connection_info: ConnectionInfo,
    db_pool: web::Data<DatabaseConnectionPool>,
    attachment_id: web::Path<Uuid>,
    attachment_data: web::Json<AttachmentUpdateData>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    if let Some(ref description) = attachment_data.description {
        validate_media_description(description)?;
    };
    let db_attachment = update_attachment(
        db_client,
        current_user.id,
        *attachment_id,
        attachment_data.description.as_deref(),
    ).await?;
    let base_url = get_request_base_url(connection_info);
    let attachment = Attachment::from_db(
        &base_url,
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
