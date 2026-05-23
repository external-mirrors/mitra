use actix_web::{
    get,
    web,
    HttpResponse,
    Scope,
};
use apx_core::{
    crypto::eddsa::{
        ed25519_public_key_from_secret_key,
        verify_eddsa_signature,
    },
};
use apx_sdk::fetch::stream_media;

use mitra_activitypub::agent::build_federation_agent;
use mitra_config::Config;
use mitra_utils::files::APPLICATION_OCTET_STREAM;

use crate::errors::HttpError;

use super::types::MediaProxyParams;

#[get("/{media_url}")]
async fn media_proxy_view(
    config: web::Data<Config>,
    media_url: web::Path<String>,
    params: web::Query<MediaProxyParams>,
) -> Result<HttpResponse, HttpError> {
    // Media URL is already decoded by path extractor
    let signature_base = media_url.as_bytes();
    let secret_key = config.instance().ed25519_secret_key;
    let public_key = ed25519_public_key_from_secret_key(&secret_key);
    verify_eddsa_signature(&public_key, signature_base, &params.signature)
        .map_err(|_| HttpError::PermissionError)?;
    let agent = build_federation_agent(&config.instance(), None);
    let supported_media_types: Vec<_> = config
        .limits
        .media
        .supported_media_types()
        .into_iter()
        // Allow application/octet-stream because some servers
        // don't properly identify the type of served content.
        .chain(vec![APPLICATION_OCTET_STREAM])
        .collect();
    let (stream, content_type) = stream_media(
        &agent,
        &media_url,
        &supported_media_types,
        config.limits.media.file_size_limit,
    ).await
        .map_err(|error| {
            log::warn!("{error}");
            // Resource can't be served at the moment
            HttpError::NotFound("media")
        })?;
    let response = HttpResponse::Ok()
        .content_type(content_type)
        .streaming(stream);
    Ok(response)
}

pub fn media_proxy_scope() -> Scope {
    web::scope("/media_proxy")
        .service(media_proxy_view)
}
