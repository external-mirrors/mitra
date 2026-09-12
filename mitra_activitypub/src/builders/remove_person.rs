use serde::Serialize;

use mitra_config::Instance;
use mitra_models::{
    accounts::types::User,
    database::{DatabaseClient, DatabaseError},
    profiles::types::DbActor,
};
use mitra_utils::id::generate_ulid;

use crate::{
    authority::Authority,
    contexts::{build_default_context, Context},
    deliverer::Recipient,
    identifiers::{
        local_activity_id,
        local_actor_id,
        LocalActorCollection,
    },
    queues::OutgoingActivityJobData,
    vocabulary::REMOVE,
};

#[derive(Serialize)]
struct RemovePerson {
    #[serde(rename = "@context")]
    _context: Context,

    #[serde(rename = "type")]
    activity_type: String,

    actor: String,
    id: String,
    object: String,
    target: String,

    to: Vec<String>,
}

fn build_remove_person(
    instance_uri: &str,
    sender_username: &str,
    person_id: &str,
    collection: LocalActorCollection,
) -> RemovePerson {
    let actor_id = local_actor_id(instance_uri, sender_username);
    let activity_id = local_activity_id(instance_uri, REMOVE, generate_ulid());
    let collection_id = collection.of(&actor_id);
    RemovePerson {
        _context: build_default_context(),
        id: activity_id,
        activity_type: REMOVE.to_string(),
        actor: actor_id,
        object: person_id.to_string(),
        target: collection_id,
        to: vec![person_id.to_string()],
    }
}

async fn prepare_remove_person(
    db_client: &impl DatabaseClient,
    instance: &Instance,
    sender: &User,
    person: &DbActor,
    collection: LocalActorCollection,
) -> Result<OutgoingActivityJobData, DatabaseError> {
    let authority = Authority::from(instance);
    let activity = build_remove_person(
        instance.uri_str(),
        &sender.profile.username,
        &person.id,
        collection,
    );
    let recipients = Recipient::for_inbox(person);
    OutgoingActivityJobData::new_outbox(
        &authority,
        db_client,
        sender,
        activity,
        recipients,
    ).await
}

pub async fn prepare_remove_subscriber(
    db_client: &impl DatabaseClient,
    instance: &Instance,
    subscription_sender: &DbActor,
    subscription_recipient: &User,
) -> Result<OutgoingActivityJobData, DatabaseError> {
    prepare_remove_person(
        db_client,
        instance,
        subscription_recipient,
        subscription_sender,
        LocalActorCollection::Subscribers,
    ).await
}

#[cfg(test)]
mod tests {
    use super::*;

    const INSTANCE_URI: &str = "https://server.example";

    #[test]
    fn test_build_remove_person() {
        let sender_username = "local";
        let person_id = "https://remote.example/actor/test";
        let collection = LocalActorCollection::Subscribers;
        let activity = build_remove_person(
            INSTANCE_URI,
            sender_username,
            person_id,
            collection,
        );

        assert_eq!(activity.activity_type, "Remove");
        assert_eq!(
            activity.actor,
            format!("{}/users/{}", INSTANCE_URI, sender_username),
        );
        assert_eq!(activity.object, person_id);
        assert_eq!(
            activity.target,
            format!("{}/users/{}/subscribers", INSTANCE_URI, sender_username),
        );
        assert_eq!(activity.to, vec![person_id]);
    }
}
