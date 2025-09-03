use chrono::DateTime;

use mitra_activitypub::identifiers::{local_actor_id, local_object_id};
use mitra_config::Instance;
use mitra_models::{
    posts::types::Post,
    profiles::types::DbActorProfile,
};
use mitra_utils::{
    html::{escape_html, extract_title},
};

use super::urls::get_user_feed_url;

const ENTRY_TITLE_MAX_LENGTH: usize = 75;

fn get_author_name(profile: &DbActorProfile) -> String {
    profile.display_name.as_ref()
        .unwrap_or(&profile.username)
        .clone()
}

fn make_entry(
    instance_url: &str,
    post: &Post,
) -> String {
    let object_id = local_object_id(instance_url, post.id);
    let content_escaped = escape_html(&post.content);
    let title = extract_title(&post.content, ENTRY_TITLE_MAX_LENGTH);
    format!(
        include_str!("templates/entry.xml"),
        url=object_id,
        title=title,
        updated_at=post.created_at.to_rfc3339(),
        author=get_author_name(&post.author),
        content=content_escaped,
    )
}

pub fn make_feed(
    instance: &Instance,
    profile: &DbActorProfile,
    posts: Vec<Post>,
) -> String {
    let actor_id = local_actor_id(&instance.url(), &profile.username);
    let feed_url = get_user_feed_url(&instance.url(), &profile.username);
    let feed_title = get_author_name(profile);
    let mut entries = vec![];
    let mut feed_updated_at = DateTime::UNIX_EPOCH;
    for post in posts {
        let entry = make_entry(&instance.url(), &post);
        entries.push(entry);
        if post.created_at > feed_updated_at {
            feed_updated_at = post.created_at;
        };
    };
    format!(
        include_str!("templates/feed.xml"),
        id=actor_id,
        url=feed_url,
        title=feed_title,
        updated_at=feed_updated_at.to_rfc3339(),
        entries=entries.join("\n"),
    )
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use uuid::uuid;
    use super::*;

    #[test]
    fn test_make_entry() {
        let instance_url = "https://social.example";
        let mut author = DbActorProfile::local_for_test("username");
        author.display_name = Some("User".to_string());
        let post_id = uuid!("67e55044-10b1-426f-9247-bb680e5fe0c8");
        let created_at = Utc.with_ymd_and_hms(2020, 3, 3, 3, 3, 3).unwrap();
        let post = Post {
            id: post_id,
            author: author,
            content: "<p>title</p><p>text text text</p>".to_string(),
            created_at: created_at,
            ..Default::default()
        };
        let entry = make_entry(instance_url, &post);
        let expected_entry = concat!(
            "<entry>\n",
            "    <id>https://social.example/objects/67e55044-10b1-426f-9247-bb680e5fe0c8</id>\n",
            "    <title>title</title>\n",
            "    <updated>2020-03-03T03:03:03+00:00</updated>\n",
            "    <author><name>User</name></author>\n",
            r#"    <content type="html">&lt;p&gt;title&lt;&#47;p&gt;&lt;p&gt;text&#32;text&#32;text&lt;&#47;p&gt;</content>"#, "\n",
            r#"    <link rel="alternate" href="https://social.example/objects/67e55044-10b1-426f-9247-bb680e5fe0c8"/>"#, "\n",
            "</entry>\n",
        );
        assert_eq!(entry, expected_entry);
    }
}
