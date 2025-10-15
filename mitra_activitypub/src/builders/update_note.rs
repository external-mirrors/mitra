use apx_core::url::http_uri::HttpUri;
use serde::Serialize;

use mitra_config::Instance;
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    posts::types::Post,
    users::types::User,
};
use mitra_services::media::MediaServer;
use mitra_utils::id::generate_ulid;

use crate::{
    authority::Authority,
    contexts::{build_default_context, Context},
    identifiers::local_activity_id,
    queues::OutgoingActivityJobData,
    vocabulary::UPDATE,
};

use super::note::{build_note, get_note_recipients, Note};

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
    instance_uri: &HttpUri,
    media_server: &MediaServer,
    post: &Post,
) -> UpdateNote {
    let authority = Authority::server(instance_uri);
    let object = build_note(
        instance_uri,
        &authority,
        media_server,
        post,
        false, // no context
    );
    let primary_audience = object.to.clone();
    let secondary_audience = object.cc.clone();
    // Update(Note) is idempotent so its ID can be random
    let activity_id = local_activity_id(
        instance_uri.as_str(),
        UPDATE,
        generate_ulid(),
    );
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
    media_server: &MediaServer,
    author: &User,
    post: &Post,
) -> Result<OutgoingActivityJobData, DatabaseError> {
    assert_eq!(author.id, post.author.id);
    let activity = build_update_note(
        instance.uri(),
        media_server,
        post,
    );
    let recipients = get_note_recipients(db_client, post).await?;
    Ok(OutgoingActivityJobData::new(
        instance.uri_str(),
        author,
        activity,
        recipients,
    ))
}

#[cfg(test)]
mod tests {
    use apx_sdk::constants::AP_PUBLIC;
    use chrono::Utc;
    use mitra_models::{
        posts::types::RelatedPosts,
        profiles::types::DbActorProfile,
    };
    use super::*;

    const INSTANCE_URI: &str = "https://social.example";

    #[test]
    fn test_build_update_note() {
        let instance_uri = HttpUri::parse(INSTANCE_URI).unwrap();
        let media_server = MediaServer::for_test(INSTANCE_URI);
        let author_username = "author";
        let author = DbActorProfile::local_for_test(author_username);
        let post = Post {
            author,
            updated_at: Some(Utc::now()),
            related_posts: Some(RelatedPosts::default()),
            ..Default::default()
        };
        let activity = build_update_note(
            &instance_uri,
            &media_server,
            &post,
        );

        assert_eq!(activity.id.starts_with(INSTANCE_URI), true);
        assert_eq!(activity.activity_type, UPDATE);
        assert_eq!(
            activity.actor,
            format!("{}/users/{}", INSTANCE_URI, author_username),
        );
        assert_eq!(activity.to, vec![AP_PUBLIC]);
        assert_eq!(activity.object._context, None);
        assert_eq!(activity.object.attributed_to, activity.actor);
        assert_eq!(activity.object.to, activity.to);
        assert_eq!(activity.object.cc, activity.cc);
        assert_eq!(activity.object.updated, post.updated_at);
    }
}
