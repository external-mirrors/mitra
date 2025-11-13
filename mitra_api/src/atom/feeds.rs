use chrono::DateTime;
use serde::Serialize;

use mitra_activitypub::identifiers::{local_actor_id, local_object_id};
use mitra_config::Instance;
use mitra_models::{
    posts::types::PostDetailed,
    profiles::types::DbActorProfile,
};
use mitra_utils::{
    html::extract_title,
};

use super::urls::get_user_feed_url;

const ENTRY_TITLE_MAX_LENGTH: usize = 75;

#[derive(Serialize)]
struct Entry {
    url: String,
    title: String,
    updated_at: String,
    author: String,
    content: String,
}

#[derive(Serialize)]
pub struct Feed {
    id: String,
    url: String,
    title: String,
    updated_at: String,
    entries: Vec<Entry>,
}

fn get_author_name(profile: &DbActorProfile) -> String {
    profile.display_name.as_ref()
        .unwrap_or(&profile.username)
        .clone()
}

fn make_entry(
    instance_uri: &str,
    post: &PostDetailed,
) -> Entry {
    let object_id = local_object_id(instance_uri, post.id);
    let title = extract_title(&post.content, ENTRY_TITLE_MAX_LENGTH);
    Entry {
        url: object_id,
        title: title,
        updated_at: post.created_at.to_rfc3339(),
        author: get_author_name(&post.author),
        content: post.content.clone(),
    }
}

pub fn make_feed(
    instance: &Instance,
    profile: &DbActorProfile,
    posts: Vec<PostDetailed>,
) -> Feed {
    let actor_id = local_actor_id(instance.uri_str(), &profile.username);
    let feed_url = get_user_feed_url(instance.uri_str(), &profile.username);
    let feed_title = get_author_name(profile);
    let mut entries = vec![];
    let mut feed_updated_at = DateTime::UNIX_EPOCH;
    for post in posts {
        let entry = make_entry(instance.uri_str(), &post);
        entries.push(entry);
        if post.created_at > feed_updated_at {
            feed_updated_at = post.created_at;
        };
    };
    Feed {
        id: actor_id,
        url: feed_url,
        title: feed_title,
        updated_at: feed_updated_at.to_rfc3339(),
        entries: entries,
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use uuid::uuid;
    use crate::templates::render_template;
    use super::*;

    #[test]
    fn test_make_feed() {
        let instance = Instance::for_test("social.example");
        let mut author = DbActorProfile::local_for_test("username");
        author.display_name = Some("User".to_string());
        let post_id = uuid!("67e55044-10b1-426f-9247-bb680e5fe0c8");
        let created_at = Utc.with_ymd_and_hms(2020, 3, 3, 3, 3, 3).unwrap();
        let post = PostDetailed {
            id: post_id,
            author: author.clone(),
            content: "<p>title</p><p>text text text</p>".to_string(),
            created_at: created_at,
            ..Default::default()
        };
        let feed_data = make_feed(&instance, &author, vec![post]);
        let feed = render_template(
            include_str!("templates/feed.xml"),
            feed_data,
        ).unwrap();
        let expected_feed = concat!(
            r#"<?xml version="1.0" encoding="utf-8"?>"#, "\n",
            r#"<feed xmlns="http://www.w3.org/2005/Atom">"#, "\n",
            "<id>https://social.example/users/username</id>", "\n",
            r#"<link rel="self" href="https://social.example/feeds/users/username"/>"#, "\n",
            "<title>User</title>", "\n",
            "<updated>2020-03-03T03:03:03+00:00</updated>", "\n",
            "<entry>\n",
            "    <id>https://social.example/objects/67e55044-10b1-426f-9247-bb680e5fe0c8</id>\n",
            "    <title>title</title>\n",
            "    <updated>2020-03-03T03:03:03+00:00</updated>\n",
            "    <author><name>User</name></author>\n",
            r#"    <content type="html">&lt;p&gt;title&lt;&#x2f;p&gt;&lt;p&gt;text text text&lt;&#x2f;p&gt;</content>"#, "\n",
            r#"    <link rel="alternate" href="https://social.example/objects/67e55044-10b1-426f-9247-bb680e5fe0c8"/>"#, "\n",
            "</entry>\n",
            "</feed>",
        );
        assert_eq!(feed, expected_feed);
    }
}
