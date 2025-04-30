use std::fmt;

use serde_json::{Value as JsonValue};

use apx_sdk::deserialization::object_to_id;
use mitra_config::Config;
use mitra_models::database::DatabaseClient;
use mitra_validators::errors::ValidationError;

use crate::{
    forwarder::get_activity_audience,
    ownership::verify_activity_owner,
    vocabulary::*,
};

use super::{
    accept::handle_accept,
    add::handle_add,
    announce::handle_announce,
    create::handle_create,
    delete::handle_delete,
    follow::handle_follow,
    like::handle_like,
    r#move::handle_move,
    offer::handle_offer,
    reject::handle_reject,
    remove::handle_remove,
    undo::handle_undo,
    update::handle_update,
    HandlerError,
};

pub enum Descriptor {
    Object(String),
    Target(String),
}

impl Descriptor {
    pub fn object(object_type: impl ToString) -> Self {
        Self::Object(object_type.to_string())
    }

    pub fn target(target_prop: &'static str) -> Self {
        Self::Target(target_prop.to_string())
    }
}

impl fmt::Display for Descriptor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Object(object) => write!(formatter, "{object}"),
            Self::Target(target) => write!(formatter, "target: {target}"),
        }
    }
}

pub async fn handle_activity(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    activity: &JsonValue,
    is_authenticated: bool,
    is_pulled: bool,
) -> Result<(), HandlerError> {
    // Validate common activity attributes
    verify_activity_owner(activity)?;
    let activity_type = activity["type"].as_str()
        .ok_or(ValidationError("type property is missing"))?
        .to_owned();
    let activity_actor = object_to_id(&activity["actor"])
        .map_err(|_| ValidationError("invalid actor property"))?;
    let _audience = get_activity_audience(activity)?;

    let activity = activity.clone();
    let maybe_descriptor = match activity_type.as_str() {
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
            handle_create(
                config,
                db_client,
                activity,
                is_authenticated,
                is_pulled,
            ).await?
        },
        DELETE => {
            handle_delete(config, db_client, activity).await?
        },
        FOLLOW => {
            handle_follow(config, db_client, activity).await?
        },
        DISLIKE | LIKE | EMOJI_REACT => {
            handle_like(config, db_client, activity).await?
        },
        LISTEN => {
            None // ignore
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
    if let Some(descriptor) = maybe_descriptor {
        log::info!(
            "processed {}({}) from {}",
            activity_type,
            descriptor,
            activity_actor,
        );
    };
    Ok(())
}
