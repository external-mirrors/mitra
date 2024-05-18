use regex::Regex;

use mitra_models::profiles::types::{
    DbActor,
    DbActorKey,
    ExtraField,
    PaymentOption,
    ProfileCreateData,
    ProfileUpdateData,
};
use mitra_utils::{
    html::{clean_html, clean_html_strict},
};

use super::{
    activitypub::validate_object_id,
    errors::ValidationError,
    posts::EMOJI_LIMIT,
};

// See also: ACTOR_ADDRESS_RE in mitra_federation::addresses
const USERNAME_RE: &str = r"^[a-zA-Z0-9_\.-]+$";
const USERNAME_LENGTH_MAX: usize = 100;
const HOSTNAME_LENGTH_MAX: usize = 100;
const DISPLAY_NAME_MAX_LENGTH: usize = 200;
const BIO_MAX_LENGTH: usize = 10000;
const BIO_ALLOWED_TAGS: [&str; 2] = ["a", "br"];
const FIELD_NAME_MAX_SIZE: usize = 500;
const FIELD_VALUE_MAX_SIZE: usize = 5000;

pub const PROFILE_IMAGE_SIZE_MAX: usize = 5 * 1000 * 1000; // 5 MB

pub fn validate_username(username: &str) -> Result<(), ValidationError> {
    if username.is_empty() {
        return Err(ValidationError("username is empty"));
    };
    if username.len() > USERNAME_LENGTH_MAX {
        return Err(ValidationError("username is too long"));
    };
    let username_regexp = Regex::new(USERNAME_RE)
        .expect("regexp should be valid");
    if !username_regexp.is_match(username) {
        return Err(ValidationError("invalid username"));
    };
    Ok(())
}

pub fn validate_hostname(hostname: &str) -> Result<(), ValidationError> {
    if hostname.len() > HOSTNAME_LENGTH_MAX {
        return Err(ValidationError("hostname is too long"));
    };
    Ok(())
}

fn validate_display_name(display_name: &str)
    -> Result<(), ValidationError>
{
    if display_name.chars().count() > DISPLAY_NAME_MAX_LENGTH {
        return Err(ValidationError("display name is too long"));
    };
    Ok(())
}

fn clean_bio(bio: &str, is_remote: bool) -> Result<String, ValidationError> {
    let cleaned_bio = if is_remote {
        // Remote profile
        let truncated_bio: String = bio.chars().take(BIO_MAX_LENGTH).collect();
        clean_html(&truncated_bio, vec![])
    } else {
        // Local profile
        if bio.chars().count() > BIO_MAX_LENGTH {
            return Err(ValidationError("bio is too long"));
        };
        clean_html_strict(bio, &BIO_ALLOWED_TAGS, vec![])
    };
    Ok(cleaned_bio)
}

pub fn allowed_profile_image_media_types(
    allowed_types: &[impl AsRef<str>],
) -> Vec<&str> {
    allowed_types
        .iter()
        .map(|media_type| media_type.as_ref())
        .filter(|media_type| media_type.starts_with("image/"))
        .collect()
}

fn validate_public_keys(
    public_keys: &[DbActorKey],
) -> Result<(), ValidationError> {
    for public_key in public_keys {
        validate_object_id(&public_key.id)?;
    };
    Ok(())
}

fn validate_payment_options(
    payment_options: &[PaymentOption],
) -> Result<(), ValidationError> {
    for payment_option in payment_options {
        if let PaymentOption::RemoteMoneroSubscription(option) = payment_option {
            validate_object_id(&option.object_id)?;
        };
    };
    Ok(())
}

/// Validates extra fields and removes fields with empty labels
fn clean_extra_fields(
    extra_fields: &[ExtraField],
    is_remote: bool,
) -> Result<Vec<ExtraField>, ValidationError> {
    let mut cleaned_extra_fields = vec![];
    for mut field in extra_fields.iter().cloned() {
        field.name = field.name.trim().to_string();
        field.value = clean_html_strict(&field.value, &BIO_ALLOWED_TAGS, vec![]);
        if field.name.is_empty() {
            continue;
        };
        if field.name.len() > FIELD_NAME_MAX_SIZE {
            return Err(ValidationError("field name is too long"));
        };
        if field.value.len() > FIELD_VALUE_MAX_SIZE {
            return Err(ValidationError("field value is too long"));
        };
        cleaned_extra_fields.push(field);
    };
    #[allow(clippy::collapsible_else_if)]
    if is_remote {
        if cleaned_extra_fields.len() > 100 {
            return Err(ValidationError("at most 100 fields are allowed"));
        };
    } else {
        if cleaned_extra_fields.len() > 10 {
            return Err(ValidationError("at most 10 fields are allowed"));
        };
    };
    Ok(cleaned_extra_fields)
}

pub fn validate_actor_data(
    actor: &DbActor,
) -> Result<(), ValidationError> {
    validate_object_id(&actor.id)?;
    validate_object_id(&actor.inbox)?;
    validate_object_id(&actor.outbox)?;
    if let Some(ref followers) = actor.followers {
        validate_object_id(followers)?;
    };
    if let Some(ref subscribers) = actor.subscribers {
        validate_object_id(subscribers)?;
    };
    if let Some(ref featured) = actor.featured {
        validate_object_id(featured)?;
    };
    Ok(())
}

pub fn clean_profile_create_data(
    profile_data: &mut ProfileCreateData,
) -> Result<(), ValidationError> {
    validate_username(&profile_data.username)?;
    if profile_data.hostname.is_some() != profile_data.actor_json.is_some() {
        return Err(ValidationError("hostname and actor_json field mismatch"));
    };
    if let Some(hostname) = &profile_data.hostname {
        validate_hostname(hostname)?;
    };
    if let Some(display_name) = &profile_data.display_name {
        validate_display_name(display_name)?;
    };
    let is_remote = if let Some(ref actor) = profile_data.actor_json {
        validate_actor_data(actor)?;
        true
    } else {
        false
    };
    if let Some(bio) = &profile_data.bio {
        let cleaned_bio = clean_bio(bio, is_remote)?;
        profile_data.bio = Some(cleaned_bio);
    };
    validate_public_keys(&profile_data.public_keys)?;
    validate_payment_options(&profile_data.payment_options)?;
    profile_data.extra_fields = clean_extra_fields(
        &profile_data.extra_fields,
        is_remote,
    )?;
    if profile_data.emojis.len() > EMOJI_LIMIT {
        return Err(ValidationError("too many emojis"));
    };
    Ok(())
}

pub fn clean_profile_update_data(
    profile_data: &mut ProfileUpdateData,
) -> Result<(), ValidationError> {
    validate_username(&profile_data.username)?;
    if let Some(display_name) = &profile_data.display_name {
        validate_display_name(display_name)?;
    };
    let is_remote = if let Some(ref actor) = profile_data.actor_json {
        validate_actor_data(actor)?;
        true
    } else {
        false
    };
    if let Some(bio) = &profile_data.bio {
        let cleaned_bio = clean_bio(bio, is_remote)?;
        profile_data.bio = Some(cleaned_bio);
    };
    validate_public_keys(&profile_data.public_keys)?;
    validate_payment_options(&profile_data.payment_options)?;
    profile_data.extra_fields = clean_extra_fields(
        &profile_data.extra_fields,
        is_remote,
    )?;
    if profile_data.emojis.len() > EMOJI_LIMIT {
        return Err(ValidationError("too many emojis"));
    };
    Ok(())
}

#[cfg(test)]
mod tests {
    use mitra_models::profiles::types::DbActor;
    use super::*;

    #[test]
    fn test_validate_username() {
        let result_1 = validate_username("test");
        assert!(result_1.is_ok());
        let result_2 = validate_username("test_12-3.xyz");
        assert!(result_2.is_ok());
    }

    #[test]
    fn test_validate_username_error() {
        let error = validate_username(&"x".repeat(101)).unwrap_err();
        assert_eq!(error.to_string(), "username is too long");
        let error = validate_username("").unwrap_err();
        assert_eq!(error.to_string(), "username is empty");
        let error = validate_username("abc&").unwrap_err();
        assert_eq!(error.to_string(), "invalid username");
    }

    #[test]
    fn test_validate_display_name() {
        let result_1 = validate_display_name("test");
        assert!(result_1.is_ok());

        let result_2 = validate_display_name(&"x".repeat(201));
        assert!(result_2.is_err());
    }

    #[test]
    fn test_clean_bio() {
        let bio = "test\n<script>alert()</script>123";
        let result = clean_bio(bio, true).unwrap();
        assert_eq!(result, "test\n123");
    }

    #[test]
    fn test_clean_extra_fields() {
        let extra_fields = vec![ExtraField {
            name: " $ETH ".to_string(),
            value: "<p>0x1234</p>".to_string(),
            value_source: None,
        }];
        let result = clean_extra_fields(&extra_fields, false)
            .unwrap().pop().unwrap();
        assert_eq!(result.name, "$ETH");
        assert_eq!(result.value, "0x1234");
    }

    #[test]
    fn test_clean_profile_create_data() {
        let mut profile_data = ProfileCreateData {
            username: "test".to_string(),
            hostname: Some("social.example".to_string()),
            display_name: Some("Test Test".to_string()),
            actor_json: Some(DbActor {
                id: "https://social.example/test".to_string(),
                inbox: "https://social.example/test/inbox".to_string(),
                outbox: "https://social.example/test/outbox".to_string(),
                ..Default::default()
            }),
            ..Default::default()
        };
        let result = clean_profile_create_data(&mut profile_data);
        assert_eq!(result.is_ok(), true);
    }
}
