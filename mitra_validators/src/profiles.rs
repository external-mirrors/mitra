use regex::Regex;

use mitra_models::profiles::types::{
    DbActor,
    DbActorKey,
    ExtraField,
    IdentityProof,
    PaymentOption,
    ProfileCreateData,
    ProfileUpdateData,
};
use mitra_utils::{
    html::{clean_html, clean_html_strict},
    urls::encode_hostname,
};

use super::{
    activitypub::{
        validate_any_object_id,
        validate_gateway_url,
    },
    errors::ValidationError,
    posts::EMOJI_LIMIT,
};

// See also: ACTOR_ADDRESS_RE in mitra_federation::addresses
const USERNAME_RE: &str = r"^[A-Za-z0-9\-\._]+$";
const USERNAME_LENGTH_MAX: usize = 100;
const HOSTNAME_RE: &str = r"^[a-z0-9\.-]+$";
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
    let normalized_hostname = encode_hostname(hostname)
        .map_err(|_| ValidationError("invalid hostname"))?;
    if normalized_hostname != hostname {
        return Err(ValidationError("hostname is not normalized"));
    };
    let hostname_re = Regex::new(HOSTNAME_RE)
        .expect("regexp should be valid");
    if !hostname_re.is_match(hostname) {
        return Err(ValidationError("invalid hostname"));
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

fn clean_bio_html(bio: &str) -> String {
    clean_html_strict(bio, &BIO_ALLOWED_TAGS, vec![])
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
        clean_bio_html(bio)
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
        validate_any_object_id(&public_key.id)?;
    };
    Ok(())
}

pub fn validate_identity_proofs(
    identity_proofs: &[IdentityProof],
) -> Result<(), ValidationError> {
    if identity_proofs.len() > 10 {
        return Err(ValidationError("at most 10 identity proofs are allowed"));
    };
    Ok(())
}

fn validate_payment_options(
    payment_options: &[PaymentOption],
) -> Result<(), ValidationError> {
    for payment_option in payment_options {
        if let PaymentOption::RemoteMoneroSubscription(option) = payment_option {
            validate_any_object_id(&option.object_id)?;
        };
    };
    Ok(())
}

pub fn clean_extra_field(field: &mut ExtraField) {
    field.name = field.name.trim().to_string();
    field.value = clean_html_strict(&field.value, &BIO_ALLOWED_TAGS, vec![]);
}

pub fn validate_extra_field(field: &ExtraField) -> Result<(), ValidationError> {
    if field.name.trim().is_empty() {
        return Err(ValidationError("field name is empty"));
    };
    if field.name.len() > FIELD_NAME_MAX_SIZE {
        return Err(ValidationError("field name is too long"));
    };
    if field.value.len() > FIELD_VALUE_MAX_SIZE {
        return Err(ValidationError("field value is too long"));
    };
    if field.value != clean_bio_html(&field.value) {
        return Err(ValidationError("field has not been sanitized"));
    };
    Ok(())
}

fn validate_extra_fields(
    extra_fields: &[ExtraField],
    is_remote: bool,
) -> Result<(), ValidationError> {
    for field in extra_fields {
        validate_extra_field(field)?;
    };
    #[allow(clippy::collapsible_else_if)]
    if is_remote {
        if extra_fields.len() > 100 {
            return Err(ValidationError("at most 100 fields are allowed"));
        };
    } else {
        if extra_fields.len() > 10 {
            return Err(ValidationError("at most 10 fields are allowed"));
        };
    };
    Ok(())
}

pub fn validate_aliases(
    identity_proofs: &[String],
) -> Result<(), ValidationError> {
    if identity_proofs.len() > 10 {
        return Err(ValidationError("at most 10 aliases are allowed"));
    };
    Ok(())
}

pub fn validate_actor_data(
    actor: &DbActor,
) -> Result<(), ValidationError> {
    validate_any_object_id(&actor.id)?;
    validate_any_object_id(&actor.inbox)?;
    validate_any_object_id(&actor.outbox)?;
    if let Some(ref followers) = actor.followers {
        validate_any_object_id(followers)?;
    };
    if let Some(ref subscribers) = actor.subscribers {
        validate_any_object_id(subscribers)?;
    };
    if let Some(ref featured) = actor.featured {
        validate_any_object_id(featured)?;
    };
    if actor.is_portable() && actor.gateways.is_empty() {
        return Err(ValidationError("at least one gateway must be specified"));
    };
    for gateway in &actor.gateways {
        validate_gateway_url(gateway)?;
    };
    Ok(())
}

pub fn clean_profile_create_data(
    profile_data: &mut ProfileCreateData,
) -> Result<(), ValidationError> {
    validate_username(&profile_data.username)?;
    if let Some(hostname) = &profile_data.hostname {
        validate_hostname(hostname)?;
    };
    if let Some(display_name) = &profile_data.display_name {
        validate_display_name(display_name)?;
    };
    let is_remote = if let Some(ref actor) = profile_data.actor_json {
        validate_actor_data(actor)?;
        if !actor.is_portable() && profile_data.hostname.is_none() {
            return Err(ValidationError(
                "non-portable remote profile should have hostname"));
        };
        true
    } else {
        false
    };
    if let Some(bio) = &profile_data.bio {
        let cleaned_bio = clean_bio(bio, is_remote)?;
        profile_data.bio = Some(cleaned_bio);
    };
    validate_public_keys(&profile_data.public_keys)?;
    validate_identity_proofs(&profile_data.identity_proofs)?;
    validate_payment_options(&profile_data.payment_options)?;
    validate_extra_fields(&profile_data.extra_fields, is_remote)?;
    validate_aliases(&profile_data.aliases)?;
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
    validate_identity_proofs(&profile_data.identity_proofs)?;
    validate_payment_options(&profile_data.payment_options)?;
    validate_extra_fields(&profile_data.extra_fields, is_remote)?;
    validate_aliases(&profile_data.aliases)?;
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
    fn test_validate_hostname() {
        let hostname = "δοκιμή.example";
        let error = validate_hostname(hostname).unwrap_err();
        assert_eq!(error.to_string(), "hostname is not normalized");

        let normalized_hostname = "xn--jxalpdlp.example";
        let result = validate_hostname(normalized_hostname);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_hostname_special_character() {
        let hostname = r#"social.example""#;
        let error = validate_hostname(hostname).unwrap_err();
        assert_eq!(error.to_string(), "invalid hostname");
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
    fn test_clean_extra_field() {
        let mut field = ExtraField {
            name: " $ETH ".to_string(),
            value: "<p>0x1234</p>".to_string(),
            value_source: None,
        };
        assert_eq!(validate_extra_field(&field).is_err(), true);
        clean_extra_field(&mut field);
        assert_eq!(field.name, "$ETH");
        assert_eq!(field.value, "0x1234");
        assert_eq!(validate_extra_field(&field).is_err(), false);
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
