use actix_web::HttpRequest;
use serde_json::{Value as JsonValue};
use wildmatch::WildMatch;

use mitra_activitypub::{
    identifiers::profile_actor_id,
};
use mitra_config::Config;
use mitra_federation::deserialization::get_object_id;
use mitra_models::database::{DatabaseClient, DatabaseError};
use mitra_utils::urls::get_hostname;
use mitra_validators::errors::ValidationError;

use crate::errors::HttpError;

use super::authentication::{
    verify_signed_activity,
    verify_signed_request,
    AuthenticationError,
};
use super::queues::IncomingActivityJobData;
use super::vocabulary::{DELETE, CREATE, LIKE, UPDATE};

#[derive(thiserror::Error, Debug)]
pub enum InboxError {
    #[error(transparent)]
    ValidationError(#[from] ValidationError),

    #[error(transparent)]
    DatabaseError(#[from] DatabaseError),

    #[error(transparent)]
    AuthError(#[from] AuthenticationError),
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
    request: &HttpRequest,
    activity: &JsonValue,
    activity_digest: [u8; 32],
) -> Result<(), InboxError> {
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
        activity_digest,
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
