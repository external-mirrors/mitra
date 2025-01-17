use mitra_models::polls::types::PollData;

use super::errors::ValidationError;

const POLL_OPTION_COUNT_MIN: usize = 2;
const POLL_OPTION_COUNT_MAX: usize = 10;
const POLL_OPTION_NAME_LENGTH_MAX: usize = 1000;

pub fn validate_poll_data(poll_data: &PollData) -> Result<(), ValidationError> {
    if poll_data.results.len() < POLL_OPTION_COUNT_MIN {
        return Err(ValidationError("too few poll options"));
    };
    if poll_data.results.len() > POLL_OPTION_COUNT_MAX {
        return Err(ValidationError("too many poll options"));
    };
    for result in &poll_data.results {
        if result.option_name.len() > POLL_OPTION_NAME_LENGTH_MAX {
            return Err(ValidationError("poll option name is too long"));
        };
    };
    Ok(())
}
