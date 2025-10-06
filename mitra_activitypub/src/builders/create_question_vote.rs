use serde::Serialize;
use serde_json::{Value as JsonValue};

use mitra_config::Instance;
use mitra_models::{
    database::DatabaseError,
    polls::types::PollVote,
    profiles::types::{DbActor, DbActorProfile},
    users::types::User,
};
use mitra_utils::id::generate_ulid;

use crate::{
    contexts::{build_default_context, Context},
    deliverer::Recipient,
    identifiers::{
        local_activity_id,
        local_actor_id,
        local_object_id,
    },
    queues::OutgoingActivityJobData,
    vocabulary::{CREATE, NOTE},
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Note {
    #[serde(rename = "type")]
    object_type: String,

    id: String,
    attributed_to: String,
    in_reply_to: String,
    name: String,
    to: String,
}

#[derive(Serialize)]
struct CreateNote {
    #[serde(rename = "@context")]
    context: Context,

    #[serde(rename = "type")]
    activity_type: String,

    id: String,
    actor: String,
    object: JsonValue,
    to: String,
}

fn build_create_question_vote(
    instance_uri: &str,
    voter: &DbActorProfile,
    question_id: &str,
    question_owner_id: &str,
    votes: Vec<PollVote>,
) -> CreateNote {
    assert!(!votes.is_empty());
    let activity_id = local_activity_id(instance_uri, CREATE, generate_ulid());
    let actor_id = local_actor_id(instance_uri, &voter.username);
    let notes: Vec<_> = votes.into_iter()
        .map(|vote| {
            let vote_id = local_object_id(instance_uri, vote.id);
            Note {
                id: vote_id,
                object_type: NOTE.to_string(),
                attributed_to: actor_id.clone(),
                in_reply_to: question_id.to_string(),
                name: vote.choice,
                to: question_owner_id.to_string(),
            }
        })
        .collect();
    let object_value = match &notes[..] {
        [note] => serde_json::to_value(note),
        _ => serde_json::to_value(notes),
    }.expect("note should be serializable");
    CreateNote {
        context: build_default_context(),
        activity_type: CREATE.to_string(),
        id: activity_id,
        actor: actor_id,
        object: object_value,
        to: question_owner_id.to_string(),
    }
}

pub fn prepare_create_question_vote(
    instance: &Instance,
    sender: &User,
    question_id: &str,
    question_owner: &DbActor,
    votes: Vec<PollVote>,
) -> Result<OutgoingActivityJobData, DatabaseError> {
    let activity = build_create_question_vote(
        instance.uri_str(),
        &sender.profile,
        question_id,
        &question_owner.id,
        votes,
    );
    let recipients = Recipient::for_inbox(question_owner);
    Ok(OutgoingActivityJobData::new(
        instance.uri_str(),
        sender,
        activity,
        recipients,
    ))
}

#[cfg(test)]
mod tests {
    use uuid::uuid;
    use super::*;

    const INSTANCE_URI: &str = "https://social.example";

    #[test]
    fn test_build_create_question_vote() {
        let voter = DbActorProfile::local_for_test("voter");
        let question_id = "https://remote.example/questions/123";
        let question_owner_id = "https://remote.example/users/test";
        let votes = vec![
            PollVote {
                id: uuid!("11fa64ff-b5a3-47bf-b23d-22b360581c3f"),
                poll_id: Default::default(),
                voter_id: Default::default(),
                choice: "1".to_string(),
                object_id: None,
            },
            PollVote {
                id: uuid!("11fa64ff-b5a3-47bf-b23d-22b360581c3e"),
                poll_id: Default::default(),
                voter_id: Default::default(),
                choice: "2".to_string(),
                object_id: None,
            },
        ];
        let activity = build_create_question_vote(
            INSTANCE_URI,
            &voter,
            question_id,
            question_owner_id,
            votes[..1].to_vec(),
        );
        assert_eq!(activity.activity_type, CREATE);
        assert_eq!(activity.actor, "https://social.example/users/voter");
        let expected_object = serde_json::json!({
            "id": "https://social.example/objects/11fa64ff-b5a3-47bf-b23d-22b360581c3f",
            "type": "Note",
            "attributedTo": "https://social.example/users/voter",
            "inReplyTo": "https://remote.example/questions/123",
            "name": "1",
            "to": "https://remote.example/users/test",
        });
        assert_eq!(activity.object, expected_object);
    }
}
