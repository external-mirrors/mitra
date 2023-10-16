use mitra_models::posts::types::{PostCreateData, PostUpdateData};
use mitra_utils::html::clean_html_strict;

use super::errors::ValidationError;

pub const ATTACHMENT_LIMIT: usize = 15;
pub const MENTION_LIMIT: usize = 50;
pub const HASHTAG_LIMIT: usize = 100;
pub const LINK_LIMIT: usize = 10;
pub const EMOJI_LIMIT: usize = 50;

pub const OBJECT_ID_SIZE_MAX: usize = 2000;
pub const CONTENT_MAX_SIZE: usize = 100000;
const CONTENT_ALLOWED_TAGS: [&str; 8] = [
    "a",
    "br",
    "pre",
    "code",
    "strong",
    "em",
    "p",
    "span",
];

pub fn content_allowed_classes() -> Vec<(&'static str, Vec<&'static str>)> {
    vec![
        ("a", vec!["hashtag", "mention", "u-url"]),
        ("span", vec!["h-card"]),
        ("p", vec!["inline-quote"]),
    ]
}

pub fn clean_local_content(
    content: &str,
) -> Result<String, ValidationError> {
    // Check content size to not exceed the hard limit
    // Character limit from config is not enforced at the backend
    if content.len() > CONTENT_MAX_SIZE {
        return Err(ValidationError("post is too long"));
    };
    let content_safe = clean_html_strict(
        content,
        &CONTENT_ALLOWED_TAGS,
        content_allowed_classes(),
    );
    let content_trimmed = content_safe.trim();
    Ok(content_trimmed.to_string())
}

pub fn validate_post_create_data(
    post_data: &PostCreateData,
) -> Result<(), ValidationError> {
    if post_data.content.is_empty() && post_data.attachments.is_empty() {
        return Err(ValidationError("post is empty"));
    };
    if post_data.attachments.len() > ATTACHMENT_LIMIT {
        return Err(ValidationError("too many attachments"));
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
    Ok(())
}

pub fn validate_post_update_data(
    post_data: &PostUpdateData,
) -> Result<(), ValidationError> {
    if post_data.content.is_empty() && post_data.attachments.is_empty() {
        return Err(ValidationError("post can not be empty"));
    };
    if post_data.attachments.len() > ATTACHMENT_LIMIT {
        return Err(ValidationError("too many attachments"));
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
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_local_content_empty() {
        let content = "  ";
        let cleaned = clean_local_content(content).unwrap();
        assert_eq!(cleaned, "");
    }

    #[test]
    fn test_clean_local_content_trimming() {
        let content = "test ";
        let cleaned = clean_local_content(content).unwrap();
        assert_eq!(cleaned, "test");
    }
}
