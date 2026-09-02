use apx_sdk::{
    constants::AP_CONTEXT,
    core::url::canonical::NonCanonicalUri,
};
use serde::Serialize;
use serde_json::{Value as JsonValue};
use uuid::Uuid;

use mitra_config::Instance;
use mitra_models::{
    accounts::types::ManagedAccount,
    database::{DatabaseClient, DatabaseError},
    profiles::types::DbActorProfile,
};
use mitra_utils::id::generate_ulid;

use crate::{
    authority::Authority,
    forwarder::Deliverable,
    identifiers::{
        local_activity_id_canonical,
        local_actor_id_canonical,
        local_object_id_canonical,
        local_quote_authorization_id,
        local_quote_authorization_path,
        profile_actor_id,
        IdBuilder,
    },
    queues::OutgoingActivityJobData,
    vocabulary::{ACCEPT, QUOTE_AUTHORIZATION},
};

#[derive(Serialize)]
struct AcceptQuoteRequest {
    #[serde(rename = "@context")]
    _context: &'static str,

    #[serde(rename = "type")]
    activity_type: &'static str,

    id: NonCanonicalUri,
    actor: NonCanonicalUri,
    // QuoteRequest
    object: JsonValue,
    // QuoteAuthorization
    result: NonCanonicalUri,
    to: Vec<NonCanonicalUri>,
}

impl Deliverable for AcceptQuoteRequest {
    fn to(&self) -> &[NonCanonicalUri] { &self.to }
    fn cc(&self) -> &[NonCanonicalUri] { &[] }
}

fn build_accept_quote_request(
    authority: &Authority,
    sender: &impl ManagedAccount,
    source: &DbActorProfile,
    quoted_object_id: Uuid,
    quoting_object_id: &str,
    request: JsonValue,
) -> AcceptQuoteRequest {
    let id_builder = authority.id_builder();
    let activity_id = local_activity_id_canonical(
        authority.root(),
        ACCEPT,
        generate_ulid(),
    );
    let actor_id = local_actor_id_canonical(
        authority.root(),
        sender.id(),
        &sender.profile().username,
    );
    let quoted_object_id = local_object_id_canonical(
        authority.root(),
        quoted_object_id,
    );
    let result_path = local_quote_authorization_path(
        &id_builder.build(&quoted_object_id).to_string(),
        &id_builder.build(&actor_id).to_string(),
        quoting_object_id,
    );
    let source_id_builder = IdBuilder::for_profile(authority, source);
    let source_id = profile_actor_id(authority, source);
    AcceptQuoteRequest {
        _context: AP_CONTEXT,
        activity_type: ACCEPT,
        id: id_builder.build(&activity_id),
        actor: id_builder.build(&actor_id),
        object: request,
        result: id_builder.build_from_path(authority.root(), result_path),
        to: vec![source_id_builder.build_unchecked(&source_id)],
    }
}

pub async fn prepare_accept_quote_request(
    db_client: &impl DatabaseClient,
    instance: &Instance,
    sender: &impl ManagedAccount,
    source: &DbActorProfile,
    quoted_object_id: Uuid,
    quoting_object_id: &str,
    request: JsonValue,
) -> Result<OutgoingActivityJobData, DatabaseError> {
    let authority = Authority::from(instance);
    let activity = build_accept_quote_request(
        &authority,
        sender,
        source,
        quoted_object_id,
        quoting_object_id,
        request,
    );
    let recipients = activity.get_recipients(db_client).await?;
    Ok(OutgoingActivityJobData::new(
        &authority,
        sender,
        activity,
        recipients,
    ))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuoteAuthorization {
    #[serde(rename = "@context")]
    _context: &'static str,

    #[serde(rename = "type")]
    object_type: &'static str,

    id: NonCanonicalUri,
    attributed_to: String,
    // accepted quote post
    interacting_object: String,
    // quoted object
    interaction_target: String,
}

pub fn build_quote_authorization(
    authority: &Authority,
    target_object_id: &str,
    target_actor_id: &str,
    interacting_object: &str,
) -> QuoteAuthorization {
    let authorization_id = local_quote_authorization_id(
        authority,
        target_object_id,
        target_actor_id,
        interacting_object,
    );
    QuoteAuthorization {
        _context: AP_CONTEXT,
        object_type: QUOTE_AUTHORIZATION,
        id: authorization_id,
        attributed_to: target_actor_id.to_owned(),
        interacting_object: interacting_object.to_owned(),
        interaction_target: target_object_id.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
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
        let source = DbActorProfile::remote_for_test("bob", source_actor_id);
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
            &sender,
            &source,
            quoted_object_id,
            quoting_object_id,
            request.clone(),
        );
        let activity = serde_json::to_value(activity).unwrap();

        assert_eq!(activity["@context"], AP_CONTEXT);
        assert_eq!(activity["type"], ACCEPT);
        assert_eq!(activity["actor"], "https://social.example/users/alice");
        assert_eq!(activity["object"], request);
        assert_eq!(activity["to"], json!([source_actor_id]));
        assert_eq!(
            activity["result"],
            "https://social.example/ap/quote-authorizations/aHR0cHM6Ly9zb2NpYWwuZXhhbXBsZS9vYmplY3RzLzAwMDAwMDAwLTAwMDAtMDAwMC0wMDAwLTAwMDAwMDAwMDAwMQ/aHR0cHM6Ly9zb2NpYWwuZXhhbXBsZS91c2Vycy9hbGljZQ/aHR0cHM6Ly9yZW1vdGUuZXhhbXBsZS9vYmplY3RzLzI",
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
        let authority = Authority::server_unchecked(INSTANCE_URI);
        let target_object_id = "https://social.example/objects/123";
        let target_actor_id = "https://social.example/users/alice";
        let interacting_object = "https://remote.example/objects/2";
        let quote_authorization = build_quote_authorization(
            &authority,
            target_object_id,
            target_actor_id,
            interacting_object,
        );
        assert_eq!(
            serde_json::to_value(quote_authorization).unwrap(),
            json!({
                "@context": AP_CONTEXT,
                "id": "https://social.example/ap/quote-authorizations/aHR0cHM6Ly9zb2NpYWwuZXhhbXBsZS9vYmplY3RzLzEyMw/aHR0cHM6Ly9zb2NpYWwuZXhhbXBsZS91c2Vycy9hbGljZQ/aHR0cHM6Ly9yZW1vdGUuZXhhbXBsZS9vYmplY3RzLzI",
                "type": "QuoteAuthorization",
                "attributedTo": target_actor_id,
                "interactingObject": interacting_object,
                "interactionTarget": target_object_id,
            }),
        );
    }
}
