use regex::Regex;

use super::errors::ValidationError;

const HASHTAG_NAME_RE: &str = r"^\w+$";
const HASHTAG_LENGTH_MAX: usize = 100;

pub fn validate_hashtag(tag_name: &str) -> Result<(), ValidationError> {
    if tag_name.len() > HASHTAG_LENGTH_MAX {
        return Err(ValidationError("tag name is too long"));
    };
    let hashtag_name_re = Regex::new(HASHTAG_NAME_RE)
        .expect("regexp should be valid");
    if !hashtag_name_re.is_match(tag_name) {
        return Err(ValidationError("invalid tag name"));
    };
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_hashtag() {
        assert!(validate_hashtag("testTag").is_ok());
        assert!(validate_hashtag("test_tag").is_ok());
        assert!(validate_hashtag("täg").is_ok());
        assert!(validate_hashtag("012").is_ok());
        assert!(validate_hashtag("#tag").is_err());
        assert!(validate_hashtag("test-tag").is_err());
        assert!(validate_hashtag("teeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeest").is_err());
    }
}
