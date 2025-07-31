use std::fmt;

use anyhow::Error;
use clap::{Parser, ValueEnum};

use mitra_models::{
    database::{get_database_client, DatabaseConnectionPool},
    filter_rules::{
        queries::{
            add_filter_rule,
            get_filter_rules,
            remove_filter_rule,
        },
        types::{FilterAction as DbFilterAction},
    },
};
use mitra_validators::filter_rules::validate_rule_target;

#[derive(Clone, ValueEnum)]
enum FilterAction {
    /// Reject incoming messages only
    RejectIncoming,
    /// Accept incoming messages
    AcceptIncoming,
    /// Reject all profiles and posts, block deliveries.
    Reject,
    /// Accept profiles and posts
    Accept,
    #[clap(hide = true)]
    RejectMedia,
    #[clap(hide = true)]
    AcceptMedia,
    /// Remove media attachments from posts
    RejectMediaAttachments,
    /// Allow media attachments
    AcceptMediaAttachments,
    /// Remove profile images
    RejectProfileImages,
    /// Allow profile images
    AcceptProfileImages,
    /// Remove custom emojis from posts and profile descriptions
    RejectCustomEmojis,
    /// Allow custom emojis
    AcceptCustomEmojis,
    /// Mark media attachments as sensitive
    MarkSensitive,
    /// Reject posts containing selected keywords
    RejectKeywords,
    /// Accept posts containing selected keywords
    AcceptKeywords,
    /// Proxy all media
    ProxyMedia,
    /// Cache all media
    CacheMedia,
}

impl FilterAction {
    fn to_db_action(&self) -> (DbFilterAction, bool) {
        match self {
            Self::RejectIncoming => (DbFilterAction::RejectIncoming, false),
            Self::AcceptIncoming => (DbFilterAction::RejectIncoming, true),
            Self::Reject => (DbFilterAction::Reject, false),
            Self::Accept => (DbFilterAction::Reject, true),
            Self::RejectMedia =>
                (DbFilterAction::RejectMediaAttachments, false),
            Self::AcceptMedia =>
                (DbFilterAction::RejectMediaAttachments, true),
            Self::RejectMediaAttachments =>
                (DbFilterAction::RejectMediaAttachments, false),
            Self::AcceptMediaAttachments =>
                (DbFilterAction::RejectMediaAttachments, true),
            Self::RejectProfileImages =>
                (DbFilterAction::RejectProfileImages, false),
            Self::AcceptProfileImages =>
                (DbFilterAction::RejectProfileImages, true),
            Self::RejectCustomEmojis =>
                (DbFilterAction::RejectCustomEmojis, false),
            Self::AcceptCustomEmojis =>
                (DbFilterAction::RejectCustomEmojis, true),
            Self::MarkSensitive =>
                (DbFilterAction::MarkSensitive, false),
            Self::RejectKeywords =>
                (DbFilterAction::RejectKeywords, false),
            Self::AcceptKeywords =>
                (DbFilterAction::RejectKeywords, true),
            Self::ProxyMedia =>
                (DbFilterAction::ProxyMedia, false),
            Self::CacheMedia =>
                (DbFilterAction::ProxyMedia, true),
        }
    }

    pub fn from_db_action(
        action: DbFilterAction,
        is_reversed: bool,
    ) -> Self {
        match (action, is_reversed) {
            (DbFilterAction::RejectIncoming, false) => Self::RejectIncoming,
            (DbFilterAction::RejectIncoming, true) => Self::AcceptIncoming,
            (DbFilterAction::Reject, false) => Self::Reject,
            (DbFilterAction::Reject, true) => Self::Accept,
            (DbFilterAction::RejectMediaAttachments, false) => Self::RejectMediaAttachments,
            (DbFilterAction::RejectMediaAttachments, true) => Self::AcceptMediaAttachments,
            (DbFilterAction::RejectProfileImages, false) => Self::RejectProfileImages,
            (DbFilterAction::RejectProfileImages, true) => Self::AcceptProfileImages,
            (DbFilterAction::RejectCustomEmojis, false) => Self::RejectCustomEmojis,
            (DbFilterAction::RejectCustomEmojis, true) => Self::AcceptCustomEmojis,
            (DbFilterAction::MarkSensitive, _) => Self::MarkSensitive,
            (DbFilterAction::RejectKeywords, false) => Self::RejectKeywords,
            (DbFilterAction::RejectKeywords, true) => Self::AcceptKeywords,
            (DbFilterAction::ProxyMedia, false) => Self::ProxyMedia,
            (DbFilterAction::ProxyMedia, true) => Self::CacheMedia,
        }
    }
}

impl fmt::Display for FilterAction {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = self.to_possible_value()
            .expect("should be convertible into PossibleValue");
        formatter.pad(value.get_name())
    }
}

/// Add federation filter rule
#[derive(Parser)]
pub struct AddFilterRule {
    /// Action to perform
    action: FilterAction,
    /// Domain name or IP address. Wildcard patterns are supported.
    target: String,
}

impl AddFilterRule {
    pub async fn execute(
        &self,
        db_pool: &DatabaseConnectionPool,
    ) -> Result<(), Error> {
        let db_client = &**get_database_client(db_pool).await?;
        validate_rule_target(&self.target)?;
        let (action, is_reversed) = self.action.to_db_action();
        add_filter_rule(
            db_client,
            &self.target,
            action,
            is_reversed,
        ).await?;
        println!("rule added");
        Ok(())
    }
}

/// Remove federation filter rule
#[derive(Parser)]
pub struct RemoveFilterRule {
    action: FilterAction,
    target: String,
}

impl RemoveFilterRule {
    pub async fn execute(
        &self,
        db_pool: &DatabaseConnectionPool,
    ) -> Result<(), Error> {
        let db_client = &**get_database_client(db_pool).await?;
        let (action, _) = self.action.to_db_action();
        remove_filter_rule(
            db_client,
            &self.target,
            action,
        ).await?;
        println!("rule removed");
        Ok(())
    }
}

/// List federation filter rules in the order of precedence.
#[derive(Parser)]
pub struct ListFilterRules;

impl ListFilterRules {
    pub async fn execute(
        &self,
        db_pool: &DatabaseConnectionPool,
    ) -> Result<(), Error> {
        let db_client = &**get_database_client(db_pool).await?;
        let rules = get_filter_rules(db_client).await?;
        for rule in rules.iter().rev() {
            let action = FilterAction::from_db_action(
                rule.filter_action,
                rule.is_reversed,
            );
            println!(
                "{0: <25} {1}",
                action,
                rule.target,
            );
        };
        Ok(())
    }
}
