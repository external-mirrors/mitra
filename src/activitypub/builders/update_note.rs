use serde::Serialize;

use mitra_config::Instance;
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    posts::types::Post,
    users::types::User,
};
use mitra_utils::id::generate_ulid;

use crate::activitypub::{
    identifiers::local_object_id,
    queues::OutgoingActivityJobData,
    types::{build_default_context, Context},
    vocabulary::UPDATE,
};

use super::create_note::{build_note, get_note_recipients, Note};

#[derive(Serialize)]
struct UpdateNote {
    #[serde(rename = "@context")]
    _context: Context,

    #[serde(rename = "type")]
    activity_type: String,

    id: String,
    actor: String,
    object: Note,

    to: Vec<String>,
    cc: Vec<String>,
}

fn build_update_note(
    instance_hostname: &str,
    instance_url: &str,
    post: &Post,
    fep_e232_enabled: bool,
) -> UpdateNote {
    let object = build_note(
        instance_hostname,
        instance_url,
        post,
        fep_e232_enabled,
        false, // no context
    );
    let primary_audience = object.to.clone();
    let secondary_audience = object.cc.clone();
    // Update(Note) is idempotent so its ID can be random
    let internal_activity_id = generate_ulid();
    let activity_id = local_object_id(instance_url, &internal_activity_id);
    UpdateNote {
        _context: build_default_context(),
        activity_type: UPDATE.to_string(),
        id: activity_id,
        actor: object.attributed_to.clone(),
        object: object,
        to: primary_audience,
        cc: secondary_audience,
    }
}

pub async fn prepare_update_note(
    db_client: &impl DatabaseClient,
    instance: &Instance,
    author: &User,
    post: &Post,
    fep_e232_enabled: bool,
) -> Result<OutgoingActivityJobData, DatabaseError> {
    assert_eq!(author.id, post.author.id);
    let activity = build_update_note(
        &instance.hostname(),
        &instance.url(),
        post,
        fep_e232_enabled,
    );
    let recipients = get_note_recipients(db_client, author, post).await?;
    Ok(OutgoingActivityJobData::new(
        author,
        activity,
        recipients,
    ))
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use mitra_models::profiles::types::DbActorProfile;
    use crate::activitypub::constants::AP_PUBLIC;
    use super::*;

    #[test]
    fn test_build_create_note() {
        let instance_hostname = "social.example";
        let instance_url = "https://social.com";
        let author_username = "author";
        let author = DbActorProfile {
            username: author_username.to_string(),
            ..Default::default()
        };
        let post = Post {
            author,
            updated_at: Some(Utc::now()),
            ..Default::default()
        };
        let activity = build_update_note(
            instance_hostname,
            instance_url,
            &post,
            false,
        );

        assert_eq!(activity.id.starts_with(instance_url), true);
        assert_eq!(activity.activity_type, UPDATE);
        assert_eq!(
            activity.actor,
            format!("{}/users/{}", instance_url, author_username),
        );
        assert_eq!(activity.to, vec![AP_PUBLIC]);
        assert_eq!(activity.object._context, None);
        assert_eq!(activity.object.attributed_to, activity.actor);
        assert_eq!(activity.object.to, activity.to);
        assert_eq!(activity.object.cc, activity.cc);
        assert_eq!(activity.object.updated, post.updated_at);
    }
}
