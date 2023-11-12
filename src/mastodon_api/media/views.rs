/// https://docs.joinmastodon.org/methods/media/#v1
use actix_web::{post, web, HttpResponse, Scope};
use actix_web_httpauth::extractors::bearer::BearerAuth;

use mitra_config::Config;
use mitra_models::{
    attachments::queries::create_attachment,
    database::{get_database_client, DbPool},
};

use crate::mastodon_api::{
    errors::MastodonError,
    oauth::auth::get_current_user,
    uploads::save_b64_file,
};
use crate::media::MediaStorage;

use super::types::{AttachmentCreateData, Attachment};

#[post("")]
async fn create_attachment_view(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<DbPool>,
    attachment_data: web::Json<AttachmentCreateData>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let media_storage = MediaStorage::from(config.as_ref());
    let (file_name, file_size, media_type) = save_b64_file(
        &attachment_data.file,
        &attachment_data.media_type,
        &media_storage,
        media_storage.file_size_limit,
        &media_storage.supported_media_types(),
    )?;
    let db_attachment = create_attachment(
        db_client,
        &current_user.id,
        file_name,
        file_size,
        media_type,
    ).await?;
    let attachment = Attachment::from_db(
        &config.instance_url(),
        db_attachment,
    );
    Ok(HttpResponse::Ok().json(attachment))
}

pub fn media_api_scope() -> Scope {
    web::scope("/api/v1/media")
        .service(create_attachment_view)
}
