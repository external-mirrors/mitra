use apx_core::url::http_uri::HttpUri;
use serde::Serialize;

use mitra_config::Instance;
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    posts::types::Post,
    users::types::User,
};
use mitra_services::media::MediaServer;

use crate::{
    authority::Authority,
    contexts::{build_default_context, Context},
    identifiers::local_activity_id,
    queues::OutgoingActivityJobData,
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
    instance_uri: &HttpUri,
    media_server: &MediaServer,
    post: &Post,
) -> CreateNote {
    let authority = Authority::server(instance_uri);
    let object = build_note(
        instance_uri,
        &authority,
        media_server,
        post,
        false,
    );
    let primary_audience = object.to.clone();
    let secondary_audience = object.cc.clone();
    let activity_id = local_activity_id(
        instance_uri.as_str(),
        CREATE,
        post.id,
    );
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
    media_server: &MediaServer,
    author: &User,
    post: &Post,
) -> Result<OutgoingActivityJobData, DatabaseError> {
    assert_eq!(author.id, post.author.id);
    let activity = build_create_note(
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
    use mitra_models::{
        posts::types::RelatedPosts,
        profiles::types::DbActorProfile,
    };
    use super::*;

    const INSTANCE_URI: &str = "https://example.com";

    #[test]
    fn test_build_create_note() {
        let instance_uri = HttpUri::parse(INSTANCE_URI).unwrap();
        let media_server = MediaServer::for_test(INSTANCE_URI);
        let author_username = "author";
        let author = DbActorProfile::local_for_test(author_username);
        let post = Post {
            author,
            related_posts: Some(RelatedPosts::default()),
            ..Default::default()
        };
        let activity = build_create_note(
            &instance_uri,
            &media_server,
            &post,
        );

        assert_eq!(
            activity.id,
            format!("{}/activities/create/{}", INSTANCE_URI, post.id),
        );
        assert_eq!(activity.activity_type, CREATE);
        assert_eq!(
            activity.actor,
            format!("{}/users/{}", INSTANCE_URI, author_username),
        );
        assert_eq!(activity.to, vec![AP_PUBLIC]);
        assert_eq!(activity.object._context, None);
        assert_eq!(activity.object.attributed_to, activity.actor);
        assert_eq!(activity.object.to, activity.to);
        assert_eq!(activity.object.cc, activity.cc);
    }
}
