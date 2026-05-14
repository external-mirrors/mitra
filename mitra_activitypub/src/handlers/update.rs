use apx_sdk::{
    authentication::{
        verify_portable_object,
        AuthenticationError,
    },
    deserialization::{deserialize_into_object_id, object_to_id},
    utils::{is_actor, is_object},
};
use serde::Deserialize;
use serde_json::{Value as JsonValue};

use mitra_models::{
    database::{
        db_client_await,
        get_database_client,
        DatabaseConnectionPool,
        DatabaseError,
    },
    invoices::{
        queries::{
            get_remote_invoice_by_object_id,
            set_invoice_status,
        },
        types::InvoiceStatus,
    },
    posts::queries::get_remote_post_by_object_id,
    profiles::queries::get_remote_profile_by_actor_id,
};
use mitra_validators::errors::ValidationError;

use crate::{
    actors::handlers::{update_remote_profile, Actor},
    builders::add_context_activity::sync_conversation,
    identifiers::canonicalize_id,
    importers::ApClient,
    ownership::verify_object_owner,
    vocabulary::AGREEMENT,
};

use super::{
    agreement::Agreement,
    note::{
        update_remote_post,
        AttributedObjectJson,
    },
    Descriptor,
    HandlerResult,
};

async fn handle_update_agreement(
    db_pool: &DatabaseConnectionPool,
    activity: JsonValue,
) -> HandlerResult {
    let agreement: Agreement =
        serde_json::from_value(activity["object"].clone())?;
    let agreement_id = agreement.id.as_ref()
        .ok_or(ValidationError("missing 'id' field"))?;
    let new_status = agreement.preview
        .ok_or(ValidationError("missing 'preview' field"))?
        .invoice_status();
    let db_client = &mut **get_database_client(db_pool).await?;
    let invoice = get_remote_invoice_by_object_id(
        db_client,
        agreement_id,
    ).await?;
    if invoice.invoice_status == InvoiceStatus::Open
        && new_status == Some(InvoiceStatus::Paid)
    {
        set_invoice_status(
            db_client,
            invoice.id,
            InvoiceStatus::Paid,
        ).await?;
    };
    Ok(Some(Descriptor::object(AGREEMENT)))
}

#[derive(Deserialize)]
struct UpdateNote {
    #[serde(deserialize_with = "deserialize_into_object_id")]
    actor: String,
    object: AttributedObjectJson,
}

async fn handle_update_note(
    ap_client: &ApClient,
    db_pool: &DatabaseConnectionPool,
    activity: JsonValue,
) -> HandlerResult {
    let update: UpdateNote = serde_json::from_value(activity.clone())?;
    let object = update.object;
    let canonical_object_id = canonicalize_id(object.id())?;
    if object.attributed_to() != update.actor {
        return Err(ValidationError("attributedTo value doesn't match actor").into());
    };
    let post = match get_remote_post_by_object_id(
        db_client_await!(db_pool),
        &canonical_object_id.to_string(),
    ).await {
        Ok(post) => post,
        // Ignore Update if post is not found locally
        Err(DatabaseError::NotFound(_)) => return Ok(None),
        Err(other_error) => return Err(other_error.into()),
    };
    let post = update_remote_post(
        ap_client,
        db_pool,
        post,
        &object,
    ).await?;
    let db_client = &**get_database_client(db_pool).await?;
    sync_conversation(
        db_client,
        &ap_client.instance,
        post.expect_conversation(),
        activity,
        post.visibility,
    ).await?;
    Ok(Some(Descriptor::object(object.inner.object_type)))
}

#[derive(Deserialize)]
struct UpdatePerson {
    actor: String,
    object: Actor,
}

async fn handle_update_person(
    ap_client: &ApClient,
    db_pool: &DatabaseConnectionPool,
    activity: JsonValue,
) -> HandlerResult {
    let update: UpdatePerson = serde_json::from_value(activity)?;
    if update.object.id() != update.actor {
        return Err(ValidationError("actor ID mismatch").into());
    };
    let canonical_actor_id = canonicalize_id(update.object.id())?;
    let profile = match get_remote_profile_by_actor_id(
        db_client_await!(db_pool),
        &canonical_actor_id.to_string(),
    ).await {
        Ok(profile) => profile,
        // Ignore Update if profile is not found locally
        Err(DatabaseError::NotFound(_)) => return Ok(None),
        Err(other_error) => return Err(other_error.into()),
    };
    let profile = update_remote_profile(
        ap_client,
        db_pool,
        profile,
        update.object,
    ).await?;
    let actor_type = &profile.expect_actor_data().object_type;
    Ok(Some(Descriptor::object(actor_type)))
}

pub async fn handle_update(
    ap_client: &ApClient,
    db_pool: &DatabaseConnectionPool,
    mut activity: JsonValue,
    is_authenticated: bool,
) -> HandlerResult {
    let is_not_embedded = activity["object"].as_str().is_some();
    if is_not_embedded || !is_authenticated {
        // Fetch object if it is not embedded or if activity is forwarded
        let object_id = object_to_id(&activity["object"])
            .map_err(|_| ValidationError("invalid activity object"))?;
        activity["object"] = ap_client.fetch_object(&object_id).await?;
        log::info!("fetched object {}", object_id);
    };
    match verify_portable_object(&activity["object"]) {
        Ok(_) => (),
        Err(AuthenticationError::InvalidObjectID(message)) => {
            return Err(ValidationError(message).into());
        },
        Err(AuthenticationError::NotPortable) => (),
        Err(other_error) => {
            log::warn!("{other_error}");
            return Err(ValidationError("invalid portable object").into());
        },
    };
    if is_actor(&activity["object"]) {
        handle_update_person(ap_client, db_pool, activity).await
    } else if is_object(&activity["object"]) {
        verify_object_owner(&activity["object"])?;
        if activity["object"]["type"].as_str() == Some(AGREEMENT) {
            handle_update_agreement(db_pool, activity).await
        } else {
            handle_update_note(ap_client, db_pool, activity).await
        }
    } else {
        log::warn!("unexpected object structure: {}", activity["object"]);
        Ok(None)
    }
}
