use actix_web::{
    http::Uri,
    HttpRequest,
};
use apx_core::{
    http_digest::ContentDigest,
    http_types::{header_map_adapter, method_adapter, uri_adapter},
    http_url_whatwg::get_hostname,
};
use apx_sdk::deserialization::object_to_id;
use serde_json::{Value as JsonValue};

use mitra_activitypub::{
    authentication::{
        verify_signed_activity,
        verify_signed_request,
        AuthenticationError,
    },
    filter::{get_moderation_domain, FederationFilter},
    identifiers::canonicalize_id,
    ownership::is_local_origin,
    queues::IncomingActivityJobData,
    vocabulary::DELETE,
};
use mitra_config::Config;
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
};
use mitra_validators::{
    errors::ValidationError,
};

use crate::{
    errors::HttpError,
};

#[derive(thiserror::Error, Debug)]
pub enum InboxError {
    #[error(transparent)]
    ValidationError(#[from] ValidationError),

    #[error(transparent)]
    DatabaseError(#[from] DatabaseError),

    #[error("{0}")]
    AuthError(#[source] AuthenticationError),
}

impl From<AuthenticationError> for InboxError {
    fn from(error: AuthenticationError) -> Self {
        match error {
            AuthenticationError::ValidationError(inner) => inner.into(),
            AuthenticationError::DatabaseError(inner) => inner.into(),
            _ => Self::AuthError(error),
        }
    }
}

impl From<InboxError> for HttpError {
    fn from(error: InboxError) -> Self {
        match error {
            InboxError::ValidationError(error) => error.into(),
            InboxError::DatabaseError(error) => error.into(),
            InboxError::AuthError(_) => {
                HttpError::AuthError("invalid signature")
            },
        }
    }
}

pub async fn receive_activity(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    request: &HttpRequest,
    request_full_uri: &Uri,
    activity: &JsonValue,
    activity_digest: ContentDigest,
    recipient_id: &str,
) -> Result<(), InboxError> {
    let activity_id = activity["id"].as_str()
        .ok_or(ValidationError("'id' property is missing"))?;
    let activity_type = activity["type"].as_str()
        .ok_or(ValidationError("'type' property is missing"))?;
    let activity_actor = object_to_id(&activity["actor"])
        .map_err(|_| ValidationError("invalid 'actor' property"))?;

    let actor_hostname = get_hostname(&activity_actor)
        .map_err(|_| ValidationError("invalid actor ID"))?;
    let filter = FederationFilter::init(config, db_client).await?;
    if filter.is_incoming_blocked(&actor_hostname) {
        log::info!("ignoring activity from blocked instance {actor_hostname}");
        return Ok(());
    };
    // Validates URIs; should be performed after filtering
    let _canonical_activity_id = canonicalize_id(activity_id)?;
    let canonical_actor_id = canonicalize_id(&activity_actor)?;

    if is_local_origin(&config.instance(), activity_id) {
        // Ignore activities with local IDs
        // and portable activities with local compatible ID.
        // Without this invalid activity might be saved and
        // served by the gateway.
        log::warn!("ignoring local activity: {activity_id}");
        return Ok(());
    };

    let is_self_delete = if activity_type == DELETE {
        let object_id = object_to_id(&activity["object"])
            .map_err(|_| ValidationError("invalid activity object"))?;
        object_id == activity_actor
    } else { false };

    // HTTP signature is required
    let mut signer = match verify_signed_request(
        config,
        db_client,
        method_adapter(request.method()),
        uri_adapter(request_full_uri),
        header_map_adapter(request.headers()),
        Some(activity_digest),
        // Don't fetch signer if this is Delete(Person) activity
        is_self_delete,
    ).await {
        Ok((_key_id, request_signer)) => {
            let request_signer_id = request_signer.expect_remote_actor_id();
            log::debug!("request signed by {}", request_signer_id);
            request_signer
        },
        Err(error) => {
            if is_self_delete && matches!(
                error,
                AuthenticationError::NoHttpSignature |
                AuthenticationError::DatabaseError(DatabaseError::NotFound(_))
            ) {
                // Ignore Delete(Person) activities without HTTP signatures
                // or if signer is not found in local database
                return Ok(());
            };
            log::warn!("invalid HTTP signature: {}", error);
            return Err(error.into());
        },
    };

    // JSON signature is optional
    // (unless the activity is portable)
    match verify_signed_activity(
        config,
        db_client,
        activity,
        // Don't fetch actor if this is Delete(Person) activity
        is_self_delete,
    ).await {
        Ok(activity_signer) => {
            let signer_id = signer.expect_remote_actor_id();
            let activity_signer_id = activity_signer.expect_remote_actor_id();
            if activity_signer_id != signer_id {
                log::warn!(
                    "request signer {} is different from activity signer {}",
                    signer_id,
                    activity_signer_id,
                );
            } else {
                log::debug!("activity signed by {}", activity_signer_id);
            };
            // Activity signature has higher priority
            signer = activity_signer;
        },
        Err(AuthenticationError::NoJsonSignature) => (), // ignore
        Err(other_error) => {
            log::warn!("invalid JSON signature: {}", other_error);
            return Err(other_error.into());
        },
    };

    let signer_hostname = get_moderation_domain(signer.expect_actor_data())?;
    if filter.is_incoming_blocked(signer_hostname.as_str()) {
        log::info!("ignoring activity from blocked instance {signer_hostname}");
        return Ok(());
    };

    let signer_id = signer.expect_remote_actor_id();
    let is_authenticated = canonical_actor_id.to_string() == signer_id;
    if !is_authenticated {
        if is_self_delete {
            // Ignore forwarded Delete(Person) activities from Mastodon
            return Ok(());
        };
        log::info!("processing forwarded {activity_type} from {signer_id}");
    };

    IncomingActivityJobData::new(
        activity,
        Some((recipient_id, signer_id)),
        is_authenticated,
    )
        .into_job(db_client, 0)
        .await?;
    log::debug!("activity added to the queue: {}", activity_type);
    Ok(())
}
