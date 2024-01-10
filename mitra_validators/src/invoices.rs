use super::errors::ValidationError;

pub fn validate_amount(
    value: u64,
) -> Result<(), ValidationError> {
    if value == 0 {
        return Err(ValidationError("amount must be greater than 0"));
    };
    i64::try_from(value)
        .map_err(|_| ValidationError("amount is too big"))?;
    Ok(())
}
