use uuid::Uuid;

use mitra_models::{
    posts::types::{
        Post,
        PostContext,
        PostCreateData,
        PostUpdateData,
        Visibility,
    },
};
use mitra_utils::html::{clean_html, clean_html_all, clean_html_strict};

use super::{
    activitypub::validate_any_object_id,
    errors::ValidationError,
    polls::validate_poll_data,
};

pub const MENTION_LIMIT: usize = 50;
pub const HASHTAG_LIMIT: usize = 100;
pub const LINK_LIMIT: usize = 10;
pub const EMOJI_LIMIT: usize = 50;

const TITLE_LENGTH_MAX: usize = 300;
const CONTENT_MAX_SIZE: usize = 100000;
const CONTENT_ALLOWED_TAGS: [&str; 12] = [
    "a",
    "br",
    "pre",
    "code",
    "strong",
    "em",
    "u",
    "del",
    "h1",
    "blockquote",
    "p",
    "span",
];
const URL_LENGTH_MAX: usize = 2000;

fn content_allowed_classes() -> Vec<(&'static str, Vec<&'static str>)> {
    vec![
        ("a", vec!["hashtag", "mention", "u-url"]),
        ("span", vec!["h-card"]),
        ("p", vec!["inline-quote"]),
    ]
}

pub fn clean_title(title: &str) -> String {
    let title = clean_html_all(title).trim().to_owned();
    let title_truncated: String = title.chars()
        .take(TITLE_LENGTH_MAX)
        .collect();
    if title_truncated.len() < title.len() {
        format!("{title_truncated}...")
    } else {
        title_truncated
    }
}

pub fn validate_content(content: &str) -> Result<(), ValidationError> {
    // Check content size to not exceed the hard limit
    // Character limit from config is not enforced at the backend
    if content.len() > CONTENT_MAX_SIZE {
        return Err(ValidationError("post is too long"));
    };
    Ok(())
}

pub fn clean_local_content(
    content: &str,
) -> String {
    let content_safe = clean_html_strict(
        content,
        &CONTENT_ALLOWED_TAGS,
        content_allowed_classes(),
    );
    let content_trimmed = content_safe.trim();
    content_trimmed.to_string()
}

pub fn clean_remote_content(content: &str) -> String {
    clean_html(content, content_allowed_classes())
}

fn validate_url(url: &str) -> Result<(), ValidationError> {
    if url.len() > URL_LENGTH_MAX {
        return Err(ValidationError("post URL is too long"));
    };
    Ok(())
}

pub fn validate_post_create_data(
    post_data: &PostCreateData,
) -> Result<(), ValidationError> {
    if let PostContext::Top { .. } = post_data.context {
        if post_data.visibility == Visibility::Conversation {
            return Err(ValidationError("top-level post can't have conversation visibility"));
        };
    };
    validate_content(&post_data.content)?;
    if post_data.content.is_empty() && post_data.attachments.is_empty() {
        return Err(ValidationError("post is empty"));
    };
    if let Some(ref poll_data) = post_data.poll {
        validate_poll_data(poll_data)?;
    };
    if post_data.mentions.len() > MENTION_LIMIT {
        return Err(ValidationError("too many mentions"));
    };
    if post_data.tags.len() > HASHTAG_LIMIT {
        return Err(ValidationError("too many hashtags"));
    };
    if post_data.links.len() > LINK_LIMIT {
        return Err(ValidationError("too many links"));
    };
    if post_data.emojis.len() > EMOJI_LIMIT {
        return Err(ValidationError("too many emojis"));
    };
    if let Some(ref url) = post_data.url {
        validate_url(url)?;
    };
    if let Some(ref object_id) = post_data.object_id {
        validate_any_object_id(object_id)?;
    };
    Ok(())
}

pub fn validate_post_update_data(
    post_data: &PostUpdateData,
) -> Result<(), ValidationError> {
    validate_content(&post_data.content)?;
    if post_data.content.is_empty() && post_data.attachments.is_empty() {
        return Err(ValidationError("post can not be empty"));
    };
    if let Some(ref poll_data) = post_data.poll {
        validate_poll_data(poll_data)?;
    };
    if post_data.mentions.len() > MENTION_LIMIT {
        return Err(ValidationError("too many mentions"));
    };
    if post_data.tags.len() > HASHTAG_LIMIT {
        return Err(ValidationError("too many hashtags"));
    };
    if post_data.links.len() > LINK_LIMIT {
        return Err(ValidationError("too many links"));
    };
    if post_data.emojis.len() > EMOJI_LIMIT {
        return Err(ValidationError("too many emojis"));
    };
    if let Some(ref url) = post_data.url {
        validate_url(url)?;
    };
    Ok(())
}

pub fn validate_post_mentions(
    mentions: &[Uuid],
    visibility: Visibility,
) -> Result<(), ValidationError> {
    if mentions.is_empty() && visibility == Visibility::Direct {
        return Err(ValidationError("direct message should have at least one mention"));
    };
    Ok(())
}

pub fn validate_local_post_links(
    links: &[Uuid],
    visibility: Visibility,
) -> Result<(), ValidationError> {
    if links.len() > 0 && visibility != Visibility::Public {
        return Err(ValidationError("can't add links to non-public posts"));
    };
    Ok(())
}

pub fn validate_reply(
    in_reply_to: &Post,
    author_id: Uuid,
    visibility: Visibility,
    mentions: &[Uuid],
) -> Result<(), ValidationError> {
     if in_reply_to.repost_of_id.is_some() {
        return Err(ValidationError("can't reply to repost"));
    };
    let is_same_author = author_id == in_reply_to.author.id;
    if !in_reply_to.visibility.can_reply_with(visibility, is_same_author) {
        return Err(ValidationError("reply must have narrower visibility"));
    };
    if in_reply_to.visibility != Visibility::Public &&
        visibility != Visibility::Public
    {
        let mut in_reply_to_audience: Vec<_> = in_reply_to.mentions.iter()
            .map(|profile| profile.id).collect();
        in_reply_to_audience.push(in_reply_to.author.id);
        if !mentions.iter().all(|id| in_reply_to_audience.contains(id)) {
            // Audience can't be expanded
            return Err(ValidationError("can't add more recipients"));
        };
    };
    Ok(())
}

#[cfg(test)]
mod tests {
    use mitra_models::profiles::types::DbActorProfile;
    use super::*;

    #[test]
    fn test_clean_title() {
        let title = "test";
        let cleaned = clean_title(title);
        assert_eq!(cleaned, title);
    }

    #[test]
    fn test_clean_title_truncate() {
        let title = "x".repeat(400);
        let cleaned = clean_title(&title);
        assert_eq!(
            cleaned,
            format!("{}...", "x".repeat(300)),
        );
    }

    #[test]
    fn test_clean_local_content_safe() {
        let content = concat!(
            r#"<p><span class="h-card"><a href="https://social.example/user" class="u-url mention">@user</a></span> test "#,
            r#"<a class="hashtag" href="https://social.example/collections/tags/tag1" rel="tag">#tag1</a> "#,
            r#"<a href="https://external.example" class="test-class">link</a> "#,
            r#"<strong class="hashtag">nottag</strong><br> "#,
            r#"<img src="https://image.example/image.png"> "#,
            r#"<script>dangerous</script></p>"#,
        );
        let cleaned_content = clean_local_content(content);
        let expected_content = concat!(
            r#"<p><span class="h-card"><a href="https://social.example/user" class="u-url mention" rel="noopener">@user</a></span> test "#,
            r#"<a class="hashtag" href="https://social.example/collections/tags/tag1" rel="tag noopener">#tag1</a> "#,
            r#"<a href="https://external.example" class="" rel="noopener">link</a> "#,
            r#"<strong>nottag</strong><br>  "#,
            r#"</p>"#,
        );
        assert_eq!(cleaned_content, expected_content);
    }

    #[test]
    fn test_clean_local_content_empty() {
        let content = "  ";
        let cleaned = clean_local_content(content);
        assert_eq!(cleaned, "");
    }

    #[test]
    fn test_clean_local_content_trimming() {
        let content = "test ";
        let cleaned = clean_local_content(content);
        assert_eq!(cleaned, "test");
    }

    #[test]
    fn test_validate_reply_wrong_visibility() {
        let author = DbActorProfile::local_for_test("author");
        let reply_author = DbActorProfile::local_for_test("author");
        let in_reply_to = Post {
            author: author.clone(),
            visibility: Visibility::Direct,
            mentions: vec![author.clone()],
            ..Default::default()
        };
        let error = validate_reply(
            &in_reply_to,
            reply_author.id,
            Visibility::Public,
            &[author.id],
        ).err().unwrap();
        assert_eq!(error.0, "reply must have narrower visibility");
    }

    #[test]
    fn test_validate_reply_adding_recipients() {
        let profile_1 = DbActorProfile::local_for_test("1");
        let profile_2 = DbActorProfile::local_for_test("2");
        let profile_3 = DbActorProfile::local_for_test("3");
        let profile_4 = DbActorProfile::local_for_test("4");
        let in_reply_to = Post {
            author: profile_1.clone(),
            visibility: Visibility::Direct,
            mentions: vec![
                profile_2.clone(),
            ],
            ..Default::default()
        };
        let error = validate_reply(
            &in_reply_to,
            profile_4.id,
            Visibility::Direct,
            &[profile_1.id, profile_3.id],
        ).err().unwrap();
        assert_eq!(error.0, "can't add more recipients");
    }
}
