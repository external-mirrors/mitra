use apx_sdk::constants::AP_CONTEXT;
use serde_json::{json, Value as JsonValue};
use uuid::Uuid;

use mitra_config::Instance;
use mitra_models::{
    accounts::types::ManagedAccount,
    database::DatabaseError,
    profiles::types::DbActor,
};
use mitra_utils::id::generate_ulid;

use crate::{
    authority::Authority,
    deliverer::Recipient,
    identifiers::{
        compatible_id,
        local_activity_id_unified,
        local_actor_id_unified,
    },
    queues::OutgoingActivityJobData,
    vocabulary::ACCEPT,
};

pub fn quote_authorization_id(
    instance_uri: &str,
    quoted_object_id: Uuid,
    actor_id: &str,
    object_id: &str,
) -> String {
    format!(
        "{instance_uri}/ap/quote-authorizations/{quoted_object_id}/{}/{}",
        hex::encode(actor_id),
        hex::encode(object_id),
    )
}

fn build_accept_quote_request(
    authority: &Authority,
    instance_uri: &str,
    sender: &impl ManagedAccount,
    source_actor_id: &str,
    quoted_object_id: Uuid,
    quoting_object_id: &str,
    request: JsonValue,
) -> JsonValue {
    let actor_id = local_actor_id_unified(
        authority,
        sender.id(),
        &sender.profile().username,
    );
    let result = quote_authorization_id(
        instance_uri,
        quoted_object_id,
        &actor_id,
        quoting_object_id,
    );
    json!({
        "@context": AP_CONTEXT,
        "id": local_activity_id_unified(authority, ACCEPT, generate_ulid()),
        "type": ACCEPT,
        "actor": actor_id,
        "object": request,
        "result": result,
        "to": [source_actor_id],
    })
}

pub fn prepare_accept_quote_request(
    instance: &Instance,
    sender: &impl ManagedAccount,
    source_actor: &DbActor,
    quoted_object_id: Uuid,
    quoting_object_id: &str,
    request: JsonValue,
) -> Result<OutgoingActivityJobData, DatabaseError> {
    let authority = Authority::from(instance);
    let source_actor_id = compatible_id(source_actor, &source_actor.id)?;
    let activity = build_accept_quote_request(
        &authority,
        instance.uri_str(),
        sender,
        &source_actor_id,
        quoted_object_id,
        quoting_object_id,
        request,
    );
    Ok(OutgoingActivityJobData::new(
        &authority,
        sender,
        activity,
        Recipient::for_inbox(source_actor),
    ))
}

pub fn build_quote_authorization(
    authorization_id: &str,
    actor_id: &str,
    object_id: &str,
    interaction_target: &str,
) -> JsonValue {
    json!({
        "@context": AP_CONTEXT,
        "id": authorization_id,
        "type": "QuoteAuthorization",
        "attributedTo": actor_id,
        "interactingObject": object_id,
        "interactionTarget": interaction_target,
    })
}

#[cfg(test)]
mod tests {
    use mitra_models::{
        accounts::types::User,
        profiles::types::DbActorProfile,
    };

    use super::*;

    const INSTANCE_URI: &str = "https://social.example";

    #[test]
    fn test_build_accept_quote_request() {
        let authority = Authority::server_unchecked(INSTANCE_URI);
        let sender = User::for_test(DbActorProfile::local_for_test("alice"));
        let source_actor_id = "https://remote.example/users/bob";
        let quoted_object_id = Uuid::from_u128(1);
        let quoted_object_uri = format!(
            "{INSTANCE_URI}/objects/{quoted_object_id}",
        );
        let quoting_object_id = "https://remote.example/objects/2";
        let request = json!({
            "id": "https://remote.example/activities/1",
            "type": "QuoteRequest",
            "actor": source_actor_id,
            "object": quoted_object_uri,
            "instrument": {
                "id": quoting_object_id,
                "type": "Note",
            },
        });

        let activity = build_accept_quote_request(
            &authority,
            INSTANCE_URI,
            &sender,
            source_actor_id,
            quoted_object_id,
            quoting_object_id,
            request.clone(),
        );

        assert_eq!(activity["@context"], AP_CONTEXT);
        assert_eq!(activity["type"], ACCEPT);
        assert_eq!(activity["actor"], "https://social.example/users/alice");
        assert_eq!(activity["object"], request);
        assert_eq!(activity["to"], json!([source_actor_id]));
        assert_eq!(
            activity["result"],
            quote_authorization_id(
                INSTANCE_URI,
                quoted_object_id,
                "https://social.example/users/alice",
                quoting_object_id,
            ),
        );
        let activity_id_prefix = format!(
            "{INSTANCE_URI}/activities/accept/",
        );
        assert!(activity["id"]
            .as_str()
            .is_some_and(|id| id.starts_with(&activity_id_prefix)));
    }

    #[test]
    fn test_build_quote_authorization() {
        let quoted_object_id = Uuid::from_u128(1);
        let actor_id = "https://social.example/users/alice";
        let object_id = "https://remote.example/objects/2";
        let interaction_target = format!(
            "{INSTANCE_URI}/objects/{quoted_object_id}",
        );
        let authorization_id = quote_authorization_id(
            INSTANCE_URI,
            quoted_object_id,
            actor_id,
            object_id,
        );

        assert_eq!(
            build_quote_authorization(
                &authorization_id,
                actor_id,
                object_id,
                &interaction_target,
            ),
            json!({
                "@context": AP_CONTEXT,
                "id": authorization_id,
                "type": "QuoteAuthorization",
                "attributedTo": actor_id,
                "interactingObject": object_id,
                "interactionTarget": interaction_target,
            }),
        );
    }
}
