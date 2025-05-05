use uuid::Uuid;

use mitra_config::PostLimits;
use mitra_validators::errors::ValidationError;

pub fn check_post_limits(
    limits: &PostLimits,
    attachments: &[Uuid],
    is_local: bool,
) -> Result<(), ValidationError> {
    let attachment_limit = if is_local {
        limits.attachment_local_limit
    } else {
        limits.attachment_limit
    };
    if attachments.len() > attachment_limit {
        return Err(ValidationError("too many attachments"));
    };
    Ok(())
}
