use apx_core::http_url::HttpUrl;
use apx_sdk::{
    authentication::{verify_portable_object, AuthenticationError},
    deserialization::deserialize_into_object_id,
    utils::is_public,
};
use serde::Deserialize;
use serde_json::{Value as JsonValue};

use mitra_config::Config;
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    filter_rules::types::FilterAction,
    properties::{
        constants::FILTER_KEYWORDS,
        queries::get_internal_property,
    },
    relationships::queries::is_local_or_followed,
};
use mitra_validators::errors::ValidationError;

use crate::{
    builders::add_context_activity::sync_conversation,
    identifiers::{
        canonicalize_id,
        parse_local_actor_id,
    },
    importers::{
        get_post_by_object_id,
        import_post,
        ApClient,
    },
    ownership::{parse_attributed_to, verify_object_owner},
};

use super::{
    note::{
        get_audience,
        get_object_content,
        AttributedObject,
        AttributedObjectJson,
    },
    question_vote::{handle_question_vote, is_question_vote},
    Descriptor,
    HandlerError,
    HandlerResult,
};

async fn check_unsolicited_message(
    db_client: &impl DatabaseClient,
    instance_url: &str,
    object: &AttributedObject,
    sender_id: &str,
) -> Result<(), HandlerError> {
    let canonical_sender_id = canonicalize_id(sender_id)?.to_string();
    // is_local_or_followed returns true if actor has local account
    let sender_has_followers =
        is_local_or_followed(db_client, &canonical_sender_id).await?;
    let audience = get_audience(object)?;
    // TODO: FEP-EF61: find portable local recipients
    let has_local_recipients = audience.iter().any(|actor_id| {
        parse_local_actor_id(instance_url, actor_id).is_ok()
    });
    // Is it a reply to a known post?
    let is_disconnected = if let Some(ref in_reply_to_id) = object.in_reply_to {
        let canonical_in_reply_to_id = canonicalize_id(in_reply_to_id)?;
        match get_post_by_object_id(
            db_client,
            instance_url,
            &canonical_in_reply_to_id,
        ).await {
            Ok(_) => false,
            Err(DatabaseError::NotFound(_)) => true,
            Err(other_error) => return Err(other_error.into()),
        }
    } else {
        true
    };
    let is_unsolicited =
        is_disconnected &&
        audience.iter().any(is_public) &&
        !has_local_recipients &&
        // Possible cause: a failure to process Undo(Follow)
        !sender_has_followers;
    if is_unsolicited {
        let error_message =
            format!("unsolicited message from {canonical_sender_id}");
        return Err(HandlerError::Filtered(error_message));
    };
    Ok(())
}

#[derive(Deserialize)]
struct CreateNote {
    #[serde(deserialize_with = "deserialize_into_object_id")]
    actor: String,
    object: JsonValue,
}

pub async fn handle_create(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    activity: JsonValue,
    maybe_sender_id: Option<&str>,
    mut is_authenticated: bool,
) -> HandlerResult {
    let CreateNote {
        actor: activity_actor,
        object,
    } = serde_json::from_value(activity.clone())?;

    let author_id = parse_attributed_to(&object["attributedTo"])?;
    if author_id != activity_actor {
        return Err(ValidationError("actor is not authorized to create object").into());
    };

    if is_question_vote(&object) && is_authenticated {
        return handle_question_vote(config, db_client, object).await;
    };
    let object: AttributedObjectJson = serde_json::from_value(object)?;
    let ap_client = ApClient::new(config, db_client).await?;
    if let Some(sender_id) = maybe_sender_id {
        check_unsolicited_message(
            db_client,
            &config.instance_url(),
            &object.inner,
            sender_id,
        ).await?;
        // TODO: FEP-EF61: keyword filtering for portable messages
        if let Ok(http_url) = HttpUrl::parse(&author_id) {
            let author_hostname = http_url.hostname();
            let content = get_object_content(&object.inner)?;
            if ap_client.filter.is_action_required(
                author_hostname.as_str(),
                FilterAction::RejectKeywords,
            ) {
                let keywords: Vec<String> = get_internal_property(
                    db_client,
                    FILTER_KEYWORDS,
                ).await?.unwrap_or_default();
                for keyword in keywords {
                    if !content.contains(&keyword) {
                        continue;
                    };
                    let error_message = format!(r#"rejected keyword "{keyword}""#);
                    return Err(HandlerError::Filtered(error_message));
                };
            };
        };
    };

    // Authentication
    match verify_portable_object(&object.value) {
        Ok(_) => {
            is_authenticated = true;
        },
        Err(AuthenticationError::InvalidObjectID(message)) => {
            return Err(ValidationError(message).into());
        },
        Err(AuthenticationError::NotPortable) => (),
        Err(other_error) => {
            log::warn!("{other_error}");
            return Err(ValidationError("invalid portable object").into());
        },
    };
    verify_object_owner(&object.value)?;

    let object_id = object.id().to_owned();
    let object_type = object.inner.object_type.clone();
    let object_received = if is_authenticated {
        Some(object)
    } else {
        // Fetch object, don't trust the sender.
        // Most likely it's a forwarded reply.
        None
    };
    let post = import_post(
        &ap_client,
        db_client,
        object_id,
        object_received,
    ).await?;
    // NOTE: import_post always returns a post; activity will be re-distributed
    sync_conversation(
        db_client,
        &ap_client.instance,
        post.expect_conversation(),
        activity,
        post.visibility,
    ).await?;
    Ok(Some(Descriptor::object(object_type)))
}
