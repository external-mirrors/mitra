use std::fmt;

use anyhow::Error;
use clap::{Parser, ValueEnum};

use mitra_models::{
    database::DatabaseClient,
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
    Reject,
    Accept,
    RejectMedia,
    AcceptMedia,
}

impl FilterAction {
    fn to_db_action(&self) -> (DbFilterAction, bool) {
        match self {
            Self::Reject => (DbFilterAction::Reject, false),
            Self::Accept => (DbFilterAction::Reject, true),
            Self::RejectMedia => (DbFilterAction::RejectMedia, false),
            Self::AcceptMedia => (DbFilterAction::RejectMedia, true),
        }
    }

    pub fn from_db_action(
        action: DbFilterAction,
        is_reversed: bool,
    ) -> Self {
        match (action, is_reversed) {
            (DbFilterAction::Reject, false) => Self::Reject,
            (DbFilterAction::Reject, true) => Self::Accept,
            (DbFilterAction::RejectMedia, false) => Self::RejectMedia,
            (DbFilterAction::RejectMedia, true) => Self::AcceptMedia,
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
    action: FilterAction,
    /// Domain name or IP address. Wildcard patterns are supported.
    target: String,
}

impl AddFilterRule {
    pub async fn execute(
        &self,
        db_client: &impl DatabaseClient,
    ) -> Result<(), Error> {
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
        db_client: &impl DatabaseClient,
    ) -> Result<(), Error> {
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
        db_client: &impl DatabaseClient,
    ) -> Result<(), Error> {
        let rules = get_filter_rules(db_client).await?;
        for rule in rules {
            let action = FilterAction::from_db_action(
                rule.filter_action,
                rule.is_reversed,
            );
            println!(
                "{0: <15} {1}",
                action,
                rule.target,
            );
        };
        Ok(())
    }
}
