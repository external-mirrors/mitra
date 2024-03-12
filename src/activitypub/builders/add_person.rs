use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use mitra_activitypub::{
    identifiers::{
        local_actor_id,
        local_agreement_id,
        local_object_id,
        LocalActorCollection,
    },
};
use mitra_config::Instance;
use mitra_models::{
    profiles::types::DbActor,
    users::types::User,
};
use mitra_utils::id::generate_ulid;

use crate::activitypub::{
    contexts::{build_default_context, Context},
    queues::OutgoingActivityJobData,
    vocabulary::ADD,
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AddPerson {
    #[serde(rename = "@context")]
    _context: Context,

    #[serde(rename = "type")]
    activity_type: String,

    actor: String,
    id: String,
    object: String,
    target: String,

    start_time: Option<DateTime<Utc>>,
    end_time: DateTime<Utc>,
    // 'context' is not used with Ethereum subscriptions
    context: Option<String>,

    to: Vec<String>,
}

fn build_add_person(
    instance_url: &str,
    sender_username: &str,
    person_id: &str,
    collection: LocalActorCollection,
    end_time: DateTime<Utc>,
    maybe_invoice_id: Option<Uuid>,
) -> AddPerson {
    let actor_id = local_actor_id(instance_url, sender_username);
    let activity_id = local_object_id(instance_url, &generate_ulid());
    let collection_id = collection.of(&actor_id);
    let maybe_context_id = maybe_invoice_id
        .map(|id| local_agreement_id(instance_url, id));
    AddPerson {
        _context: build_default_context(),
        id: activity_id,
        activity_type: ADD.to_string(),
        actor: actor_id,
        object: person_id.to_string(),
        target: collection_id,
        start_time: None,
        end_time: end_time,
        context: maybe_context_id,
        to: vec![person_id.to_string()],
    }
}

pub fn prepare_add_person(
    instance: &Instance,
    sender: &User,
    person: &DbActor,
    collection: LocalActorCollection,
    end_time: DateTime<Utc>,
    maybe_invoice_id: Option<Uuid>,
) -> OutgoingActivityJobData {
    let activity = build_add_person(
        &instance.url(),
        &sender.profile.username,
        &person.id,
        collection,
        end_time,
        maybe_invoice_id,
    );
    let recipients = vec![person.clone()];
    OutgoingActivityJobData::new(
        sender,
        activity,
        recipients,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const INSTANCE_URL: &str = "https://social.example";

    #[test]
    fn test_build_add_person() {
        let sender_username = "local";
        let person_id = "https://remote.example/actor/test";
        let collection = LocalActorCollection::Subscribers;
        let invoice_id = generate_ulid();
        let subscription_expires_at = Utc::now();
        let activity = build_add_person(
            INSTANCE_URL,
            sender_username,
            person_id,
            collection,
            subscription_expires_at,
            Some(invoice_id),
        );

        assert_eq!(activity.activity_type, "Add");
        assert_eq!(
            activity.actor,
            format!("{}/users/{}", INSTANCE_URL, sender_username),
        );
        assert_eq!(activity.object, person_id);
        assert_eq!(
            activity.target,
            format!("{}/users/{}/subscribers", INSTANCE_URL, sender_username),
        );
        assert_eq!(
            activity.context.as_ref().unwrap(),
            &format!("{}/objects/agreements/{}", INSTANCE_URL, invoice_id),
        );
        assert_eq!(activity.start_time.is_none(), true);
        assert_eq!(activity.end_time, subscription_expires_at);
        assert_eq!(activity.to[0], person_id);
    }
}
