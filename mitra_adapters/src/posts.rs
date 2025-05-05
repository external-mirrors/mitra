use uuid::Uuid;

use mitra_config::PostLimits;
use mitra_validators::errors::ValidationError;

pub fn check_post_limits(
    limits: &PostLimits,
    attachments: &[Uuid],
) -> Result<(), ValidationError> {
    if attachments.len() > limits.attachment_limit {
        return Err(ValidationError("too many attachments"));
    };
    Ok(())
}
