use mitra_models::polls::types::PollData;

use super::errors::ValidationError;

const POLL_OPTION_COUNT_MIN: usize = 2;
pub const POLL_OPTION_COUNT_MAX: usize = 10;
pub const POLL_OPTION_NAME_LENGTH_MAX: usize = 1000;

pub fn validate_poll_data(poll_data: &PollData) -> Result<(), ValidationError> {
    if poll_data.results.len() < POLL_OPTION_COUNT_MIN {
        return Err(ValidationError("too few poll options"));
    };
    if poll_data.results.len() > POLL_OPTION_COUNT_MAX {
        return Err(ValidationError("too many poll options"));
    };
    let mut unique_options = vec![];
    for result in &poll_data.results {
        if result.option_name.len() > POLL_OPTION_NAME_LENGTH_MAX {
            return Err(ValidationError("poll option name is too long"));
        };
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
