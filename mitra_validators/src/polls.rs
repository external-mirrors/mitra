use mitra_models::polls::types::PollData;
use mitra_utils::html::clean_html_all;

use super::errors::ValidationError;

const POLL_OPTION_COUNT_MIN: usize = 2;
// https://github.com/mastodon/mastodon/blob/v4.3.8/app/validators/poll_options_validator.rb
pub const POLL_OPTION_COUNT_MAX: usize = 15;
pub const POLL_OPTION_NAME_LENGTH_MAX: usize = 1000;

pub fn clean_poll_option_name(name: &str) -> String {
    clean_html_all(name)
}

fn validate_poll_option_name(option_name: &str) -> Result<(), ValidationError> {
    if option_name.len() > POLL_OPTION_NAME_LENGTH_MAX {
        return Err(ValidationError("poll option name is too long"));
    };
    if option_name != clean_poll_option_name(option_name) {
        return Err(ValidationError("option name has not been sanitized"));
    };
    Ok(())
}

pub fn validate_poll_data(poll_data: &PollData) -> Result<(), ValidationError> {
    if poll_data.results.len() < POLL_OPTION_COUNT_MIN {
        return Err(ValidationError("too few poll options"));
    };
    if poll_data.results.len() > POLL_OPTION_COUNT_MAX {
        return Err(ValidationError("too many poll options"));
    };
    let mut unique_options = vec![];
    for result in &poll_data.results {
        validate_poll_option_name(&result.option_name)?;
        if !unique_options.contains(&&result.option_name) {
            unique_options.push(&result.option_name);
        } else {
            return Err(ValidationError("poll options must be unique"));
        };
    };
    Ok(())
}

#[cfg(test)]
mod tests {
    use mitra_models::polls::types::PollResult;
    use super::*;

    #[test]
    fn test_validate_poll_option_name_sanitized() {
        let name = "test <span>html</span>";
        assert_eq!(validate_poll_option_name(name).is_ok(), false);
    }

    #[test]
    fn test_poll_data_unique_options() {
        let poll_data = PollData {
            multiple_choices: false,
            ends_at: Default::default(),
            results: vec![PollResult::new("a"), PollResult::new("a")],
        };
        let result = validate_poll_data(&poll_data);
        assert_eq!(
            result.err().unwrap().0,
            "poll options must be unique",
        );
    }
}
