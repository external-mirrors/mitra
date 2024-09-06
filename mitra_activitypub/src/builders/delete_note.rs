use serde::Serialize;

use mitra_config::Instance;
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    posts::helpers::add_related_posts,
    posts::types::Post,
    users::types::User,
};

use crate::{
    authority::Authority,
    contexts::{build_default_context, Context},
    identifiers::{local_activity_id, local_actor_id, local_object_id},
    queues::OutgoingActivityJobData,
    vocabulary::{DELETE, NOTE, TOMBSTONE},
};

use super::note::{build_note, get_note_recipients, Note};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Tombstone {
    id: String,

    #[serde(rename = "type")]
    object_type: String,

    former_type: String,
}

#[derive(Serialize)]
struct DeleteNote {
    #[serde(rename = "@context")]
    context: Context,

    #[serde(rename = "type")]
    activity_type: String,

    id: String,
    actor: String,
    object: Tombstone,

    to: Vec<String>,
    cc: Vec<String>,
}

fn build_delete_note(
    instance_hostname: &str,
    instance_url: &str,
    post: &Post,
    fep_e232_enabled: bool,
) -> DeleteNote {
    assert!(post.is_local());
    let object_id = local_object_id(instance_url, post.id);
    let activity_id = local_activity_id(instance_url, DELETE, post.id);
    let actor_id = local_actor_id(instance_url, &post.author.username);
    let authority = Authority::server(instance_url);
    let Note { to, cc, .. } = build_note(
        instance_hostname,
        instance_url,
        &authority,
        post,
        fep_e232_enabled,
        false,
    );
    DeleteNote {
        context: build_default_context(),
        activity_type: DELETE.to_string(),
        id: activity_id,
        actor: actor_id,
        object: Tombstone {
            id: object_id,
            object_type: TOMBSTONE.to_string(),
            former_type: NOTE.to_string(),
        },
        to: to,
        cc: cc,
    }
}

pub async fn prepare_delete_note(
    db_client: &impl DatabaseClient,
    instance: &Instance,
    author: &User,
    post: &Post,
    fep_e232_enabled: bool,
) -> Result<OutgoingActivityJobData, DatabaseError> {
    assert_eq!(author.id, post.author.id);
    let mut post = post.clone();
    add_related_posts(db_client, vec![&mut post]).await?;
    let activity = build_delete_note(
        &instance.hostname(),
        &instance.url(),
        &post,
        fep_e232_enabled,
    );
    let recipients = get_note_recipients(db_client, author, &post).await?;
    Ok(OutgoingActivityJobData::new(
        &instance.url(),
        author,
        activity,
        recipients,
    ))
}

#[cfg(test)]
mod tests {
    use apx_sdk::constants::AP_PUBLIC;
    use mitra_models::profiles::types::DbActorProfile;
    use super::*;

    const INSTANCE_HOSTNAME: &str = "example.com";
    const INSTANCE_URL: &str = "https://example.com";

    #[test]
    fn test_build_delete_note() {
        let author = DbActorProfile::local_for_test("author");
        let post = Post { author, ..Default::default() };
        let activity = build_delete_note(
            INSTANCE_HOSTNAME,
            INSTANCE_URL,
            &post,
            false,
        );

        assert_eq!(
            activity.id,
            format!("{}/activities/delete/{}", INSTANCE_URL, post.id),
        );
        assert_eq!(
            activity.object.id,
            format!("{}/objects/{}", INSTANCE_URL, post.id),
        );
        assert_eq!(activity.object.object_type, "Tombstone");
        assert_eq!(activity.to, vec![AP_PUBLIC]);
        assert_eq!(
            activity.cc,
            vec![format!("{INSTANCE_URL}/users/author/followers")],
        );
    }
}
