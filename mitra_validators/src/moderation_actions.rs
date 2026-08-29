use super::errors::ValidationError;

const REASON_LENGTH_MAX: usize = 1000;

pub fn validate_action_reason(reason: &str) -> Result<(), ValidationError> {
    if reason.chars().count() > REASON_LENGTH_MAX {
        return Err(ValidationError("action reason is too long"));
    };
    Ok(())
}
