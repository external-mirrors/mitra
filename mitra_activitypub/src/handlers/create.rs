use serde::Deserialize;
use serde_json::{Value as JsonValue};

use apx_sdk::{
    authentication::{verify_portable_object, AuthenticationError},
    deserialization::deserialize_into_object_id,
    utils::is_public,
};
use mitra_config::Config;
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    relationships::queries::has_local_followers,
};
use mitra_services::media::MediaStorage;
use mitra_validators::errors::ValidationError;

use crate::{
    filter::FederationFilter,
    identifiers::{
        canonicalize_id,
        parse_local_actor_id,
    },
    importers::{
        get_post_by_object_id,
        import_post,
    },
    ownership::verify_object_owner,
};

use super::{
    note::{
        get_audience,
        get_object_attributed_to,
        AttributedObject,
        AttributedObjectJson,
    },
    Descriptor,
    HandlerError,
    HandlerResult,
};

async fn check_unsolicited_message(
    db_client: &impl DatabaseClient,
    instance_url: &str,
    object: &AttributedObject,
) -> Result<(), HandlerError> {
    let author_id = get_object_attributed_to(object)?;
    let canonical_author_id = canonicalize_id(&author_id)?.to_string();
    let author_has_followers =
        has_local_followers(db_client, &canonical_author_id).await?;
    let audience = get_audience(object);
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
        !author_has_followers;
    if is_unsolicited {
        return Err(HandlerError::UnsolicitedMessage(canonical_author_id));
    };
    Ok(())
}

#[derive(Deserialize)]
struct CreateNote {
    #[serde(deserialize_with = "deserialize_into_object_id")]
    actor: String,
    object: AttributedObjectJson,
}

pub async fn handle_create(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    activity: JsonValue,
    mut is_authenticated: bool,
    is_pulled: bool,
) -> HandlerResult {
    let CreateNote {
        actor: activity_actor,
        object,
    } = serde_json::from_value(activity)?;

    if !is_pulled {
        check_unsolicited_message(
            db_client,
            &config.instance_url(),
            &object.inner,
        ).await?;
    };

    let author_id = get_object_attributed_to(&object.inner)?;
    if author_id != activity_actor {
        return Err(ValidationError("actor is not authorized to create object").into());
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
        Err(_) => {
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
    let filter = FederationFilter::init(config, db_client).await?;
    import_post(
        db_client,
        &filter,
        &config.instance(),
        &MediaStorage::from(config),
        object_id,
        object_received,
    ).await?;
    Ok(Some(Descriptor::object(object_type)))
}
