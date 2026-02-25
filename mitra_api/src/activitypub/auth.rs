use actix_web::{
    http::Uri,
    HttpRequest,
};
use apx_core::http_types::{header_map_adapter, method_adapter, uri_adapter};

use mitra_activitypub::{
    authentication::{
        verify_signed_request,
    },
    importers::ApClient,
};
use mitra_models::{
    database::DatabaseConnectionPool,
    profiles::types::DbActorProfile,
};

use super::receiver::EndpointError;

pub async fn check_request(
    ap_client: &ApClient,
    db_pool: &DatabaseConnectionPool,
    request: &HttpRequest,
    request_full_uri: &Uri,
) -> Result<DbActorProfile, EndpointError> {
    let signer = match verify_signed_request(
        ap_client,
        db_pool,
        method_adapter(request.method()),
        uri_adapter(request_full_uri),
        header_map_adapter(request.headers()),
        None, // GET request has no content
        true, // don't fetch actor
    ).await {
        Ok((_, signer)) => signer,
        Err(error) => {
            log::warn!("request verification error: {error}");
            // Will be converted into HttpError
            return Err(error.into());
        },
    };
    Ok(signer)
}
