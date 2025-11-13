use uuid::Uuid;

use mitra_config::PostLimits;
use mitra_validators::{
    common::Origin,
    errors::ValidationError,
};

pub fn check_post_limits(
    limits: &PostLimits,
    attachments: &[Uuid],
    origin: Origin,
) -> Result<(), ValidationError> {
    let attachment_limit = match origin {
        Origin::Local => limits.attachment_local_limit,
        Origin::Remote => limits.attachment_limit,
    };
    if attachments.len() > attachment_limit {
        return Err(ValidationError("too many attachments"));
    };
    Ok(())
}
