use mitra_models::groups::types::GroupCreateData;

use crate::{
    accounts::validate_local_username,
    errors::ValidationError,
    profiles::{clean_bio, validate_bio},
};

pub fn clean_group_create_data(group_data: &mut GroupCreateData) -> () {
    if let Some(ref bio) = group_data.bio {
        let cleaned_bio = clean_bio(bio, false);
        group_data.bio = Some(cleaned_bio);
    };
}

pub fn validate_group_create_data(
    group_data: &GroupCreateData,
) -> Result<(), ValidationError> {
    validate_local_username(&group_data.username)?;
    if let Some(ref bio) = group_data.bio {
        validate_bio(bio)?;
    };
    Ok(())
}
