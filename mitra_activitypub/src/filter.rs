use wildmatch::WildMatch;

use apx_core::{
    http_url::Hostname,
};
use mitra_config::Config;
use mitra_models::{
    database::{DatabaseClient, DatabaseError, DatabaseTypeError},
    filter_rules::{
        queries::get_filter_rules,
        types::{FilterRule, FilterAction},
    },
    profiles::types::DbActor,
};

use crate::utils::parse_http_url_from_db;

// Returns DatabaseError if actor data is not valid
// TODO: validation should happen during actor data deserialization
pub fn get_moderation_domain(
    actor: &DbActor,
) -> Result<Hostname, DatabaseError> {
    let http_url = if actor.is_portable() {
        // TODO: return None if gateway list is empty
        actor.gateways.first().ok_or(DatabaseTypeError)?
    } else {
        &actor.id
    };
    let hostname = parse_http_url_from_db(http_url)?.hostname();
    Ok(hostname)
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

pub struct FederationFilter {
    blocklist: Vec<String>,
    allowlist: Vec<String>,
    rules: Vec<FilterRule>,
}

impl FederationFilter {
    pub async fn init(
        config: &Config,
        db_client: &impl DatabaseClient,
    ) -> Result<Self, DatabaseError> {
        let rules = get_filter_rules(db_client).await?;
        Ok(Self {
            blocklist: config.blocked_instances.clone(),
            allowlist: config.allowed_instances.clone(),
            rules,
        })
    }

    pub fn is_action_required(
        &self,
        hostname: &str,
        action: FilterAction,
    ) -> bool {
        // Rules are checked in order. The last matching rule wins.
        let mut is_required = false;
        // Blocklist and allowlist have lower priority than filter rules
        if action == FilterAction::RejectIncoming {
            is_required = !is_hostname_allowed(
                &self.blocklist,
                &self.allowlist,
                hostname,
            );
        };
        let applicable_rules = self.rules.iter()
            .filter(|rule| WildMatch::new(&rule.target).matches(hostname))
            .filter(|rule| rule.filter_action == action);
        // Apply rules, starting with less specific
        for rule in applicable_rules {
            is_required = !rule.is_reversed;
        };
        is_required
    }

    pub fn is_incoming_blocked(&self, hostname: &str) -> bool {
        self.is_action_required(hostname, FilterAction::RejectIncoming) ||
        self.is_action_required(hostname, FilterAction::Reject)
    }
}

#[cfg(test)]
mod tests {
    use serial_test::serial;
    use mitra_models::{
        database::test_utils::create_test_database,
        filter_rules::queries::add_filter_rule,
    };
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

    #[tokio::test]
    #[serial]
    async fn test_federation_filter() {
        let db_client = &create_test_database().await;
        let target_1 = "*";
        let target_2 = "one.example";
        let target_3 = "two.example";
        add_filter_rule(
            db_client,
            target_1,
            FilterAction::RejectIncoming,
            false, // block
        ).await.unwrap();
        add_filter_rule(
            db_client,
            target_2,
            FilterAction::RejectIncoming,
            true, // allow
        ).await.unwrap();
        let rules = get_filter_rules(db_client).await.unwrap();

        let filter = FederationFilter {
            blocklist: vec![],
            allowlist: vec![target_3.to_string()], // overridden
            rules,
        };
        assert_eq!(filter.is_incoming_blocked("one.example"), false);
        assert_eq!(filter.is_incoming_blocked("two.example"), true);
        assert_eq!(filter.is_incoming_blocked("any.example"), true);
    }
}
