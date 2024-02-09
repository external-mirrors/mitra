use serde::Serialize;

use mitra_config::Instance;
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    posts::types::Post,
    users::types::User,
};

use crate::activitypub::{
    queues::OutgoingActivityJobData,
    types::{build_default_context, Context},
    vocabulary::CREATE,
};

use super::note::{build_note, get_note_recipients, Note};

#[derive(Serialize)]
pub struct CreateNote {
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

pub fn build_create_note(
    instance_hostname: &str,
    instance_url: &str,
    post: &Post,
    fep_e232_enabled: bool,
) -> CreateNote {
    let object = build_note(
        instance_hostname,
        instance_url,
        post,
        fep_e232_enabled,
        false,
    );
    let primary_audience = object.to.clone();
    let secondary_audience = object.cc.clone();
    let activity_id = format!("{}/create", object.id);
    CreateNote {
        _context: build_default_context(),
        activity_type: CREATE.to_string(),
        id: activity_id,
        actor: object.attributed_to.clone(),
        object: object,
        to: primary_audience,
        cc: secondary_audience,
    }
}

pub async fn prepare_create_note(
    db_client: &impl DatabaseClient,
    instance: &Instance,
    author: &User,
    post: &Post,
    fep_e232_enabled: bool,
) -> Result<OutgoingActivityJobData, DatabaseError> {
    assert_eq!(author.id, post.author.id);
    let activity = build_create_note(
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
    use mitra_federation::constants::AP_PUBLIC;
    use mitra_models::profiles::types::DbActorProfile;
    use super::*;

    const INSTANCE_HOSTNAME: &str = "example.com";
    const INSTANCE_URL: &str = "https://example.com";

    #[test]
    fn test_build_create_note() {
        let author_username = "author";
        let author = DbActorProfile {
            username: author_username.to_string(),
            ..Default::default()
        };
        let post = Post { author, ..Default::default() };
        let activity = build_create_note(
            INSTANCE_HOSTNAME,
            INSTANCE_URL,
            &post,
            false,
        );

        assert_eq!(
            activity.id,
            format!("{}/objects/{}/create", INSTANCE_URL, post.id),
        );
        assert_eq!(activity.activity_type, CREATE);
        assert_eq!(
            activity.actor,
            format!("{}/users/{}", INSTANCE_URL, author_username),
        );
        assert_eq!(activity.to, vec![AP_PUBLIC]);
        assert_eq!(activity.object._context, None);
        assert_eq!(activity.object.attributed_to, activity.actor);
        assert_eq!(activity.object.to, activity.to);
        assert_eq!(activity.object.cc, activity.cc);
    }
}
