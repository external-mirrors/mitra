use apx_sdk::constants::AP_PUBLIC;
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use mitra_config::Instance;
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    posts::types::{Post, Visibility},
    relationships::queries::get_followers,
    users::types::User,
};

use crate::{
    contexts::{build_default_context, Context},
    deliverer::Recipient,
    identifiers::{
        local_activity_id,
        local_actor_id,
        local_object_id,
        post_object_id,
        profile_actor_id,
        LocalActorCollection,
    },
    queues::OutgoingActivityJobData,
    vocabulary::ANNOUNCE,
};

#[derive(Serialize)]
pub struct Announce {
    #[serde(rename = "@context")]
    context: Context,

    #[serde(rename = "type")]
    activity_type: String,

    id: String,
    actor: String,
    object: String,
    published: DateTime<Utc>,

    to: Vec<String>,
    cc: Vec<String>,
}

pub(super) fn local_announce_activity_id(
    instance_uri: &str,
    repost_id: Uuid,
    repost_has_deprecated_ap_id: bool,
) -> String {
    if repost_has_deprecated_ap_id {
        local_object_id(instance_uri, repost_id)
    } else {
        local_activity_id(instance_uri, ANNOUNCE, repost_id)
    }
}

pub(super) fn get_announce_audience(
    visibility: Visibility,
    actor_id: &str,
    recipient_id: &str,
) -> (Vec<String>, Vec<String>) {
    let mut primary_audience = vec![];
    let mut secondary_audience = vec![];
    let actor_followers = LocalActorCollection::Followers.of(actor_id);
    match visibility {
        Visibility::Public => {
            primary_audience.push(AP_PUBLIC.to_owned());
            secondary_audience.push(actor_followers);
        },
        Visibility::Followers => {
            primary_audience.push(actor_followers);
        },
        _ => (),
    };
    primary_audience.push(recipient_id.to_owned());
    (primary_audience, secondary_audience)
}

pub fn build_announce(
    instance_uri: &str,
    repost: &Post,
) -> Announce {
    let actor_id = local_actor_id(instance_uri, &repost.author.username);
    let post = repost
        .expect_related_posts()
        .repost_of.as_ref()
        .expect("repost_of field should be populated");
    let object_id = post_object_id(instance_uri, post);
    let activity_id = local_announce_activity_id(instance_uri, repost.id, false);
    let recipient_id = profile_actor_id(instance_uri, &post.author);
    let (primary_audience, secondary_audience) = get_announce_audience(
        repost.visibility,
        &actor_id,
        &recipient_id,
    );
    Announce {
        context: build_default_context(),
        activity_type: ANNOUNCE.to_string(),
        actor: actor_id,
        id: activity_id,
        object: object_id,
        published: repost.created_at,
        to: primary_audience,
        cc: secondary_audience,
    }
}

pub async fn get_announce_recipients(
    db_client: &impl DatabaseClient,
    current_user: &User,
    repost_visibility: Visibility,
    post: &Post,
) -> Result<Vec<Recipient>, DatabaseError> {
    let mut recipients = vec![];
    match repost_visibility {
        Visibility::Public | Visibility::Followers => {
            let followers = get_followers(db_client, current_user.id).await?;
            for profile in followers {
                if let Some(remote_actor) = profile.actor_json {
                    recipients.extend(Recipient::for_inbox(&remote_actor));
                };
            };
        },
        _ => (),
    };
    if let Some(remote_actor) = post.author.actor_json.as_ref() {
        recipients.extend(Recipient::for_inbox(remote_actor));
    };
    Ok(recipients)
}

pub async fn prepare_announce(
    db_client: &impl DatabaseClient,
    instance: &Instance,
    sender: &User,
    repost: &Post,
) -> Result<OutgoingActivityJobData, DatabaseError> {
    assert_eq!(sender.id, repost.author.id);
    let post = repost
        .expect_related_posts()
        .repost_of.as_ref()
        .expect("repost_of field should be populated");
    let recipients = get_announce_recipients(
        db_client,
        sender,
        repost.visibility,
        post,
    ).await?;
    let activity = build_announce(
        instance.uri_str(),
        repost,
    );
    Ok(OutgoingActivityJobData::new(
        instance.uri_str(),
        sender,
        activity,
        recipients,
    ))
}

#[cfg(test)]
mod tests {
    use mitra_models::{
        profiles::types::DbActorProfile,
        posts::types::RelatedPosts,
    };
    use super::*;

    const INSTANCE_URI: &str = "https://example.com";

    #[test]
    fn test_build_announce() {
        let post_author_id = "https://test.net/user/test";
        let post_author = DbActorProfile::remote_for_test(
            "test",
            post_author_id,
        );
        let post_id = "https://test.net/obj/123";
        let post = Post {
            author: post_author.clone(),
            object_id: Some(post_id.to_string()),
            ..Default::default()
        };
        let repost_author = DbActorProfile::local_for_test("announcer");
        let repost = Post {
            author: repost_author,
            repost_of_id: Some(post.id),
            related_posts: Some(RelatedPosts {
                repost_of: Some(Box::new(post)),
                ..Default::default()
            }),
            ..Default::default()
        };
        let activity = build_announce(
            INSTANCE_URI,
            &repost,
        );
        assert_eq!(
            activity.id,
            format!("{}/activities/announce/{}", INSTANCE_URI, repost.id),
        );
        assert_eq!(
            activity.actor,
            format!("{}/users/announcer", INSTANCE_URI),
        );
        assert_eq!(activity.object, post_id);
        assert_eq!(activity.to, vec![AP_PUBLIC, post_author_id]);
        assert_eq!(
            activity.cc,
            vec![format!("{INSTANCE_URI}/users/announcer/followers")],
        );
    }
}
