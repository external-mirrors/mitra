use regex::Regex;

use super::errors::ValidationError;
use super::profiles::validate_username;

const USERNAME_RE: &str = r"^[a-z0-9_]+$";
// Same as Mastodon's limit
// https://github.com/mastodon/mastodon/blob/4b9e4f6398760cc04f9fde2c659f30ffea216e12/app/models/account.rb#L91
const USERNAME_LENGTH_MAX: usize = 30;

pub fn validate_local_username(username: &str) -> Result<(), ValidationError> {
    validate_username(username)?;
    // The username regexp should not allow domain names and IP addresses
    let username_regexp = Regex::new(USERNAME_RE).unwrap();
    if !username_regexp.is_match(username) {
        return Err(ValidationError("invalid username"));
    };
    if username.len() > USERNAME_LENGTH_MAX {
        return Err(ValidationError("username is too long"));
    };
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_local_username() {
        let result_1 = validate_local_username("name_1");
        assert_eq!(result_1.is_ok(), true);
        let result_2 = validate_local_username("name&");
        assert_eq!(result_2.is_ok(), false);
        let result_3 = validate_local_username(&"a".repeat(55));
        assert_eq!(result_3.is_ok(), false);
    }
}
