use regex::Regex;

use mitra_models::profiles::types::Origin;

use super::errors::ValidationError;

// https://github.com/mastodon/mastodon/blob/v4.3.7/app/models/custom_emoji.rb#L30
const EMOJI_LOCAL_NAME_RE: &str = r"^[a-zA-Z0-9_]{2,}$";
const EMOJI_REMOTE_NAME_RE: &str = r"^[a-zA-Z0-9._+-]+$";
pub (super) const EMOJI_NAME_SIZE_MAX: usize = 100; // database column limit

// https://github.com/mastodon/mastodon/blob/v4.4.2/app/models/custom_emoji.rb#L37
pub const EMOJI_LOCAL_MEDIA_TYPES: [&str; 3] = [
    "image/gif",
    "image/png",
    "image/webp",
];
pub const EMOJI_REMOTE_MEDIA_TYPES: [&str; 6] = [
    "image/apng",
    "image/avif",
    "image/gif",
    "image/jpeg",
    "image/png",
    "image/webp",
];

pub fn validate_emoji_name(
    emoji_name: &str,
    origin: Origin,
) -> Result<(), ValidationError> {
    let name_re = match origin {
        Origin::Local => Regex::new(EMOJI_LOCAL_NAME_RE)
            .expect("regexp should be valid"),
        Origin::Remote => Regex::new(EMOJI_REMOTE_NAME_RE)
            .expect("regexp should be valid"),
    };
    if !name_re.is_match(emoji_name) {
        return Err(ValidationError("invalid emoji name"));
    };
    if emoji_name.len() > EMOJI_NAME_SIZE_MAX {
        return Err(ValidationError("emoji name is too long"));
    };
    Ok(())
}

pub(super) fn parse_emoji_shortcode(shortcode: &str) -> Option<&str> {
    shortcode.strip_prefix(':')
        .and_then(|val| val.strip_suffix(':'))
}

pub fn clean_emoji_name(emoji_name: &str) -> &str {
    if let Some(emoji_name) = parse_emoji_shortcode(emoji_name) {
        emoji_name
    } else {
        emoji_name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_emoji_name_local() {
        let valid_name = "emoji_name";
        let result = validate_emoji_name(valid_name, Origin::Local);
        assert!(result.is_ok());

        let valid_name = "01-emoji-name";
        let result = validate_emoji_name(valid_name, Origin::Local);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_emoji_name_remote() {
        let valid_name = "emoji_name";
        let result = validate_emoji_name(valid_name, Origin::Remote);
        assert!(result.is_ok());

        let valid_name = "01-emoji-name";
        let result = validate_emoji_name(valid_name, Origin::Remote);
        assert!(result.is_ok());

        let invalid_name = "emoji\"<script>";
        let result = validate_emoji_name(invalid_name, Origin::Remote);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_emoji_shortcode() {
        let result = parse_emoji_shortcode("test_emoji");
        assert_eq!(result, None);
        let result = parse_emoji_shortcode(":test_emoji");
        assert_eq!(result, None);
        let result = parse_emoji_shortcode("test_emoji:");
        assert_eq!(result, None);
        let result = parse_emoji_shortcode(":test_emoji:");
        assert_eq!(result, Some("test_emoji"));
    }

    #[test]
    fn test_clean_emoji_name() {
        let emoji_name = "test_emoji";
        let output = clean_emoji_name(emoji_name);
        assert_eq!(output, "test_emoji");
        let shortcode = ":test_emoji:";
        let output = clean_emoji_name(shortcode);
        assert_eq!(output, "test_emoji");
    }

    #[test]
    fn test_clean_emoji_name_invalid_shortcode() {
        let shortcode = "test_emoji:";
        let output = clean_emoji_name(shortcode);
        assert_eq!(output, "test_emoji:");
    }
}
