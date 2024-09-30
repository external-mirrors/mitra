use actix_web::HttpRequest;
use serde_json::{Value as JsonValue};
use wildmatch::WildMatch;

use apx_core::urls::get_hostname;
use apx_sdk::deserialization::get_object_id;
use mitra_activitypub::{
    identifiers::canonicalize_id,
    queues::IncomingActivityJobData,
    vocabulary::{ANNOUNCE, DELETE, CREATE, LIKE, UPDATE},
};
use mitra_config::Config;
use mitra_models::{
    activitypub::queries::{
        add_object_to_collection,
        save_activity,
    },
    database::{DatabaseClient, DatabaseError},
    users::types::PortableUser,
};
use mitra_validators::{
    errors::ValidationError,
};

use crate::errors::HttpError;

use super::authentication::{
    verify_signed_activity,
    verify_signed_request,
    AuthenticationError,
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
            AuthenticationError::DatabaseError(db_error) => db_error.into(),
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
    maybe_fep_ef61_recipient: Option<&PortableUser>,
    request: &HttpRequest,
    activity: &JsonValue,
    activity_digest: [u8; 32],
) -> Result<(), InboxError> {
    let activity_id = activity["id"].as_str()
        .ok_or(ValidationError("'id' property is missing"))?;
    let activity_type = activity["type"].as_str()
        .ok_or(ValidationError("'type' property is missing"))?;
    let activity_actor = get_object_id(&activity["actor"])
        .map_err(|_| ValidationError("invalid 'actor' property"))?;

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
    // Validates URIs; should be performed after filtering
    let canonical_activity_id = canonicalize_id(activity_id)?;
    let canonical_actor_id = canonicalize_id(&activity_actor)?;

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
        Some(activity_digest),
        // Don't fetch signer if this is Delete(Person) activity
        is_self_delete,
    ).await {
        Ok(request_signer) => {
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

    let signer_id = signer.expect_remote_actor_id();
    // TODO: FEP-EF61: implement instance blocking for portable actors
    if !signer.is_portable() {
        let signer_hostname = get_hostname(signer_id)
            .map_err(|_| ValidationError("invalid actor ID"))?;
        if !is_hostname_allowed(
            &config.blocked_instances,
            &config.allowed_instances,
            &signer_hostname,
        ) {
            log::info!("ignoring activity from blocked instance {signer_hostname}");
            return Ok(());
        };
    };

    let is_authenticated = canonical_actor_id == signer_id;
    if !is_authenticated {
        match activity_type {
            CREATE | UPDATE => {
                // Accept forwarded Create() and Update() activities
                log::info!("processing forwarded {activity_type} activity");
            },
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

    // Save authenticated activities to database
    if is_authenticated {
        let is_new = save_activity(
            db_client,
            &canonical_activity_id,
            activity,
        ).await?;
        if let Some(recipient) = maybe_fep_ef61_recipient {
            add_object_to_collection(
                db_client,
                recipient.id,
                &recipient.profile.expect_actor_data().inbox,
                &canonical_activity_id,
            ).await?;
        };
        if !is_new {
            if activity_type == ANNOUNCE {
                // Optimization for FEP-1b12.
                // Activity will be processed only once,
                // even if submitted to multiple inboxes
                log::warn!("skipping repeated activity: {canonical_activity_id}");
                return Ok(());
            } else {
                log::info!("repeated activity: {canonical_activity_id}");
            };
        };
    };

    // Add activity to job queue and release lock
    IncomingActivityJobData::new(activity, is_authenticated)
        .into_job(db_client, 0)
        .await?;
    log::debug!("activity added to the queue: {}", activity_type);
    Ok(())
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
