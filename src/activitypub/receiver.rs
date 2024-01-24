use actix_web::HttpRequest;
use serde_json::{Value as JsonValue};
use wildmatch::WildMatch;

use mitra_config::Config;
use mitra_federation::{
    deserialization::get_object_id,
    fetch::FetchError,
};
use mitra_models::database::{DatabaseClient, DatabaseError};
use mitra_services::media::MediaStorageError;
use mitra_utils::urls::get_hostname;
use mitra_validators::errors::ValidationError;

use crate::errors::HttpError;

use super::authentication::{
    verify_signed_activity,
    verify_signed_request,
    AuthenticationError,
};
use super::handlers::{
    accept::handle_accept,
    add::handle_add,
    announce::handle_announce,
    create::{
        handle_create,
        validate_create,
    },
    delete::handle_delete,
    follow::handle_follow,
    like::handle_like,
    r#move::handle_move,
    offer::handle_offer,
    reject::handle_reject,
    remove::handle_remove,
    undo::handle_undo,
    update::handle_update,
};
use super::identifiers::profile_actor_id;
use super::queues::IncomingActivityJobData;
use super::vocabulary::*;

#[derive(thiserror::Error, Debug)]
pub enum HandlerError {
    #[error("local object")]
    LocalObject,

    #[error(transparent)]
    FetchError(#[from] FetchError),

    #[error(transparent)]
    ValidationError(#[from] ValidationError),

    #[error(transparent)]
    DatabaseError(#[from] DatabaseError),

    #[error("media storage error")]
    StorageError(#[from] MediaStorageError),

    #[error("{0}")]
    ServiceError(&'static str),

    #[error(transparent)]
    AuthError(#[from] AuthenticationError),

    #[error("unsolicited message from {0}")]
    UnsolicitedMessage(String),
}

impl From<HandlerError> for HttpError {
    fn from(error: HandlerError) -> Self {
        match error {
            HandlerError::LocalObject => HttpError::InternalError,
            HandlerError::FetchError(error) => {
                HttpError::ValidationError(error.to_string())
            },
            HandlerError::ValidationError(error) => error.into(),
            HandlerError::DatabaseError(error) => error.into(),
            HandlerError::StorageError(_) => HttpError::InternalError,
            HandlerError::ServiceError(_) => HttpError::InternalError,
            HandlerError::AuthError(_) => {
                HttpError::AuthError("invalid signature")
            },
            // Return 403 Forbidden
            HandlerError::UnsolicitedMessage(_) => HttpError::PermissionError,
        }
    }
}

pub async fn handle_activity(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    activity: &JsonValue,
    is_authenticated: bool,
) -> Result<(), HandlerError> {
    let activity_type = activity["type"].as_str()
        .ok_or(ValidationError("type property is missing"))?
        .to_owned();
    let activity_actor = get_object_id(&activity["actor"])
        .map_err(|_| ValidationError("invalid actor property"))?;
    let activity = activity.clone();
    let maybe_object_type = match activity_type.as_str() {
        ACCEPT => {
            handle_accept(config, db_client, activity).await?
        },
        ADD => {
            handle_add(config, db_client, activity).await?
        },
        ANNOUNCE => {
            handle_announce(config, db_client, activity).await?
        },
        CREATE => {
            handle_create(config, db_client, activity, is_authenticated).await?
        },
        DELETE => {
            handle_delete(config, db_client, activity).await?
        },
        FOLLOW => {
            handle_follow(config, db_client, activity).await?
        },
        LIKE | EMOJI_REACT => {
            handle_like(config, db_client, activity).await?
        },
        MOVE => {
            handle_move(config, db_client, activity).await?
        },
        OFFER => {
            handle_offer(config, db_client, activity).await?
        },
        REJECT => {
            handle_reject(config, db_client, activity).await?
        },
        REMOVE => {
            handle_remove(config, db_client, activity).await?
        },
        UNDO => {
            handle_undo(config, db_client, activity).await?
        },
        UPDATE => {
            handle_update(config, db_client, activity, is_authenticated).await?
        },
        _ => {
            log::warn!("activity type is not supported: {}", activity);
            None
        },
    };
    if let Some(object_type) = maybe_object_type {
        log::info!(
            "processed {}({}) from {}",
            activity_type,
            object_type,
            activity_actor,
        );
    };
    Ok(())
}

fn is_hostname_allowed(
    blocklist: &[String],
    allowlist: &[String],
    hostname: &str,
) -> bool {
    if blocklist.iter()
        .any(|blocked| WildMatch::new(blocked).matches(hostname))
    {
        // Blocked, checking allowlist
        allowlist.iter()
            .any(|allowed| WildMatch::new(allowed).matches(hostname))
    } else {
        true
    }
}

pub async fn receive_activity(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    request: &HttpRequest,
    activity: &JsonValue,
) -> Result<(), HandlerError> {
    let activity_type = activity["type"].as_str()
        .ok_or(ValidationError("type property is missing"))?;
    let activity_actor = get_object_id(&activity["actor"])
        .map_err(|_| ValidationError("invalid actor property"))?;

    let actor_hostname = get_hostname(&activity_actor)
        .map_err(|_| ValidationError("invalid actor ID"))?;
    if !is_hostname_allowed(
        &config.blocked_instances,
        &config.allowed_instances,
        &actor_hostname,
    ) {
        log::info!("ignoring activity from blocked instance {actor_hostname}");
        return Ok(());
    };

    let is_self_delete = if activity_type == DELETE {
        let object_id = get_object_id(&activity["object"])
            .map_err(|_| ValidationError("invalid activity object"))?;
        object_id == activity_actor
    } else { false };

    // HTTP signature is required
    let mut signer = match verify_signed_request(
        config,
        db_client,
        request,
        // Don't fetch signer if this is Delete(Person) activity
        is_self_delete,
    ).await {
        Ok(request_signer) => {
            log::debug!("request signed by {}", request_signer.acct);
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
    match verify_signed_activity(
        config,
        db_client,
        activity,
        // Don't fetch actor if this is Delete(Person) activity
        is_self_delete,
    ).await {
        Ok(activity_signer) => {
            if activity_signer.acct != signer.acct {
                log::warn!(
                    "request signer {} is different from activity signer {}",
                    signer.acct,
                    activity_signer.acct,
                );
            } else {
                log::debug!("activity signed by {}", activity_signer.acct);
            };
            // Activity signature has higher priority
            signer = activity_signer;
        },
        Err(AuthenticationError::NoJsonSignature) => (), // ignore
        Err(other_error) => {
            log::warn!("invalid JSON signature: {}", other_error);
        },
    };

    let signer_hostname = signer.hostname.as_ref()
        .expect("signer should be remote");
    if !is_hostname_allowed(
        &config.blocked_instances,
        &config.allowed_instances,
        signer_hostname,
    ) {
        log::info!("ignoring activity from blocked instance {signer_hostname}");
        return Ok(());
    };

    let signer_id = profile_actor_id(&config.instance_url(), &signer);
    let is_authenticated = activity_actor == signer_id;
    if !is_authenticated {
        match activity_type {
            CREATE | UPDATE => (), // Accept forwarded Create() and Update() activities
            DELETE | LIKE => {
                // Ignore forwarded Delete and Like activities
                return Ok(());
            },
            _ => {
                // Reject other types
                log::warn!(
                    "request signer {} does not match actor {}",
                    signer_id,
                    activity_actor,
                );
                return Err(AuthenticationError::UnexpectedSigner.into());
            },
        };
    };

    if activity_type == CREATE {
        // Validate before putting into the queue
        validate_create(config, db_client, activity).await?;
    };

    if let ANNOUNCE | CREATE | DELETE | MOVE | UNDO | UPDATE = activity_type {
        // Add activity to job queue and release lock
        IncomingActivityJobData::new(activity, is_authenticated)
            .into_job(db_client, 0).await?;
        log::debug!("activity added to the queue: {}", activity_type);
        return Ok(());
    };

    handle_activity(
        config,
        db_client,
        activity,
        is_authenticated,
    ).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_hostname_allowed() {
        let blocklist = vec!["bad.example".to_string()];
        let allowlist = vec![];
        let result = is_hostname_allowed(&blocklist, &allowlist, "social.example");
        assert_eq!(result, true);
        let result = is_hostname_allowed(&blocklist, &allowlist, "bad.example");
        assert_eq!(result, false);
    }

    #[test]
    fn test_is_hostname_allowed_wildcard() {
        let blocklist = vec!["*.eu".to_string()];
        let allowlist = vec![];
        let result = is_hostname_allowed(&blocklist, &allowlist, "social.example");
        assert_eq!(result, true);
        let result = is_hostname_allowed(&blocklist, &allowlist, "social.eu");
        assert_eq!(result, false);
    }

    #[test]
    fn test_is_hostname_allowed_allowlist() {
        let blocklist = vec!["*".to_string()];
        let allowlist = vec!["social.example".to_string()];
        let result = is_hostname_allowed(&blocklist, &allowlist, "social.example");
        assert_eq!(result, true);
        let result = is_hostname_allowed(&blocklist, &allowlist, "other.example");
        assert_eq!(result, false);
    }
}
