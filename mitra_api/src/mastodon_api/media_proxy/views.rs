use actix_web::{
    get,
    web,
    HttpResponse,
    Scope,
};
use apx_core::{
    crypto_eddsa::{
        ed25519_public_key_from_secret_key,
        verify_eddsa_signature,
    },
};
use apx_sdk::{
    core::url::common::url_decode,
    fetch::fetch_file_streaming,
};

use mitra_activitypub::agent::build_federation_agent;
use mitra_config::Config;

use crate::errors::HttpError;

use super::types::MediaProxyParams;

#[get("/{url_encoded}")]
async fn media_proxy_view(
    config: web::Data<Config>,
    url_encoded: web::Path<String>,
    params: web::Query<MediaProxyParams>,
) -> Result<HttpResponse, HttpError> {
    let url = url_decode(&url_encoded);
    let signature_base = url.as_bytes();
    let secret_key = config.instance().ed25519_secret_key;
    let public_key = ed25519_public_key_from_secret_key(&secret_key);
    verify_eddsa_signature(&public_key, signature_base, &params.signature)
        .map_err(|_| HttpError::PermissionError)?;
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
