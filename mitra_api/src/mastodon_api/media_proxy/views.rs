use actix_web::{
    get,
    web,
    HttpResponse,
    Scope,
};
use apx_sdk::{
    core::url::common::url_decode,
    fetch::fetch_file_streaming,
};

use mitra_activitypub::agent::build_federation_agent;
use mitra_config::Config;

use crate::errors::HttpError;

#[get("/{url_encoded}")]
async fn media_proxy_view(
    config: web::Data<Config>,
    url_encoded: web::Path<String>,
) -> Result<HttpResponse, HttpError> {
    let url = url_decode(&url_encoded);
    let agent = build_federation_agent(&config.instance(), None);
    let (stream, content_type) = fetch_file_streaming(
        &agent,
        &url,
        &config.limits.media.supported_media_types(),
        config.limits.media.file_size_limit,
    ).await
        .map_err(HttpError::from_internal)?;
    let response = HttpResponse::Ok()
        .content_type(content_type)
        .streaming(stream);
    Ok(response)
}

pub fn media_proxy_scope() -> Scope {
    web::scope("/media_proxy")
        .service(media_proxy_view)
}
