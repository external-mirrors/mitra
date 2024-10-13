use wildmatch::WildMatch;

use apx_core::{
    http_url::{HttpUrl, Hostname},
};
use mitra_models::{
    database::{DatabaseError, DatabaseTypeError},
    profiles::types::DbActor,
};

// Return DatabaseError if actor data is not valid
// TODO: validation should happen during actor data deserialization
pub fn get_moderation_domain(
    actor: &DbActor,
) -> Result<Hostname, DatabaseError> {
    let http_url = if actor.is_portable() {
        actor.gateways.first().ok_or(DatabaseTypeError)?
    } else {
        &actor.id
    };
    let hostname = HttpUrl::parse(http_url)
        .map_err(|_| DatabaseTypeError)?
        .hostname();
    Ok(hostname)
}

pub fn is_hostname_allowed(
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
