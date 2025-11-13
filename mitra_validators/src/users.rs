use regex::Regex;

use mitra_models::{
    users::types::ClientConfig,
};

use super::errors::ValidationError;
use super::profiles::validate_username;

const USERNAME_RE: &str = r"^[A-Za-z0-9_\-]+$";
// Same as Mastodon's limit
// https://github.com/mastodon/mastodon/blob/4b9e4f6398760cc04f9fde2c659f30ffea216e12/app/models/account.rb#L91
const USERNAME_LENGTH_MAX: usize = 30;
const CLIENT_CONFIG_SIZE_MAX: usize = 20 * 1000;

pub fn validate_local_username(username: &str) -> Result<(), ValidationError> {
    validate_username(username)?;
    // The username regexp should not allow domain names and IP addresses
    let username_regexp = Regex::new(USERNAME_RE)
        .expect("regexp should be valid");
    if !username_regexp.is_match(username) {
        return Err(ValidationError("only letters, numbers, hyphens, and underscores are allowed in usernames"));
    };
    if username.len() > USERNAME_LENGTH_MAX {
        return Err(ValidationError("username is too long"));
    };
    Ok(())
}

fn client_config_size(config: &ClientConfig) -> usize {
    serde_json::to_string(config)
        .expect("client config should be serializable")
        .len()
}

pub fn validate_client_config_update(
    config: &ClientConfig,
    update: &ClientConfig,
) -> Result<(), ValidationError> {
    if update.len() != 1 {
        return Err(ValidationError("can't update more than one config"));
    };
    let expected_config_size =
        client_config_size(config) + client_config_size(update);
    if expected_config_size > CLIENT_CONFIG_SIZE_MAX {
        return Err(ValidationError("client config size exceeds limit"));
    };
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use serde_json::json;
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

    #[test]
    fn test_validate_client_config_update() {
        let config = HashMap::new();
        let update = HashMap::from([
            ("test_client".to_string(), json!({"test": "value"})),
        ]);
        let result = validate_client_config_update(&config, &update);
        assert_eq!(result.is_ok(), true);
    }
}
