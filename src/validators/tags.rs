use regex::Regex;

use crate::errors::ValidationError;

const HASHTAG_NAME_RE: &str = r"^\w+$";

pub fn validate_hashtag(tag_name: &str) -> Result<(), ValidationError> {
    let hashtag_name_re = Regex::new(HASHTAG_NAME_RE).unwrap();
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
    }
}
