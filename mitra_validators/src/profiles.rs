use regex::Regex;

use apx_core::{
    url::hostname::encode_hostname,
};
use mitra_models::profiles::types::{
    DbActor,
    DbActorKey,
    ExtraField,
    IdentityProof,
    PaymentOption,
    ProfileCreateData,
    ProfileUpdateData,
    WebfingerHostname,
};
use mitra_utils::{
    html::{clean_html, clean_html_all, clean_html_strict},
};

use super::{
    activitypub::{
        validate_any_object_id,
        validate_endpoint_url,
        validate_gateway_url,
        validate_origin,
    },
    errors::ValidationError,
    posts::EMOJI_LIMIT,
};

// See also: WEBFINGER_ADDRESS_RE in apx_sdk::addresses
const USERNAME_RE: &str = r"^[A-Za-z0-9\-\._]+$";
const USERNAME_LENGTH_MAX: usize = 100;
const HOSTNAME_RE: &str = r"^[a-z0-9\.-]+$";
const HOSTNAME_LENGTH_MAX: usize = 100;
const DISPLAY_NAME_MAX_LENGTH: usize = 200;
const BIO_MAX_LENGTH: usize = 10000;
const BIO_ALLOWED_TAGS: [&str; 3] = [
    "a",
    "br",
    "p",
];
pub const FIELD_LOCAL_LIMIT: usize = 10;
pub const FIELD_REMOTE_LIMIT: usize = 100;
pub const FIELD_NAME_LENGTH_MAX: usize = 500;
pub const FIELD_VALUE_LENGTH_MAX: usize = 5000;
const FIELD_ALLOWED_TAGS: [&str; 1] = ["a"];
pub const ALIAS_LIMIT: usize = 10;

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

fn clean_display_name_html(display_name: &str) -> String {
    clean_html_all(display_name).replace("&nbsp;", " ")
}

fn clean_display_name(display_name: &str, is_remote: bool) -> String {
    let mut text = clean_display_name_html(display_name);
    if is_remote {
        text = text.chars().take(DISPLAY_NAME_MAX_LENGTH).collect();
    };
    text
}

fn validate_display_name(display_name: &str)
    -> Result<(), ValidationError>
{
    if display_name.chars().count() > DISPLAY_NAME_MAX_LENGTH {
        return Err(ValidationError("display name is too long"));
    };
    if display_name != clean_display_name_html(display_name) {
        return Err(ValidationError("display name has not been sanitized"));
    };
    Ok(())
}

fn clean_bio_html(bio: &str) -> String {
    clean_html_strict(bio, &BIO_ALLOWED_TAGS, vec![])
}

fn clean_bio(bio: &str, is_remote: bool) -> String {
    if is_remote {
        // Remote profile
        let truncated_bio: String = bio.chars().take(BIO_MAX_LENGTH).collect();
        clean_html(&truncated_bio, vec![])
    } else {
        // Local profile
        clean_bio_html(bio)
    }
}

fn validate_bio(bio: &str) -> Result<(), ValidationError> {
    if bio.chars().count() > BIO_MAX_LENGTH {
            return Err(ValidationError("bio is too long"));
        };
    if bio != clean_html(bio, vec![]) {
        return Err(ValidationError("bio has not been sanitized"));
    };
    Ok(())
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

fn clean_extra_field_value(value: &str) -> String {
    clean_html_strict(value, &FIELD_ALLOWED_TAGS, vec![])
}

pub fn clean_extra_field(field: &mut ExtraField) {
    field.name = field.name.trim().to_string();
    field.value = clean_extra_field_value(&field.value);
}

pub fn validate_extra_field(field: &ExtraField) -> Result<(), ValidationError> {
    if field.name.trim().is_empty() {
        return Err(ValidationError("field name is empty"));
    };
    if field.name.len() > FIELD_NAME_LENGTH_MAX {
        return Err(ValidationError("field name is too long"));
    };
    if field.value.len() > FIELD_VALUE_LENGTH_MAX {
        return Err(ValidationError("field value is too long"));
    };
    if field.value != clean_extra_field_value(&field.value) {
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
        if extra_fields.len() > FIELD_REMOTE_LIMIT {
            return Err(ValidationError("at most 100 fields are allowed"));
        };
    } else {
        if extra_fields.len() > FIELD_LOCAL_LIMIT {
            return Err(ValidationError("at most 10 fields are allowed"));
        };
    };
    Ok(())
}

pub fn validate_aliases(
    aliases: &[String],
) -> Result<(), ValidationError> {
    if aliases.len() > ALIAS_LIMIT {
        return Err(ValidationError("at most 10 aliases are allowed"));
    };
    Ok(())
}

pub fn validate_actor_data(
    actor: &DbActor,
) -> Result<(), ValidationError> {
    validate_any_object_id(&actor.id)?;
    validate_any_object_id(&actor.inbox)?;
    if let Some(ref shared_inbox) = actor.shared_inbox {
        validate_endpoint_url(shared_inbox)?;
    };
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
    if actor.is_portable() {
        validate_origin(&actor.id, &actor.inbox)?;
        validate_origin(&actor.id, &actor.outbox)?;
        if actor.gateways.is_empty() {
            return Err(ValidationError("at least one gateway must be specified"));
        };
    };
    for gateway in &actor.gateways {
        validate_gateway_url(gateway)?;
    };
    Ok(())
}

fn validate_profile_create_data(
    profile_data: &ProfileCreateData,
) -> Result<(), ValidationError> {
    validate_username(&profile_data.username)?;
    if let WebfingerHostname::Remote(ref hostname) = profile_data.hostname {
        validate_hostname(hostname)?;
    };
    if let Some(display_name) = &profile_data.display_name {
        validate_display_name(display_name)?;
    };
    let is_remote = if let Some(ref actor) = profile_data.actor_json {
        validate_actor_data(actor)?;
        if !actor.is_portable() && profile_data.hostname.as_str().is_none() {
            return Err(ValidationError(
                "non-portable remote profile should have hostname"));
        };
        true
    } else {
        false
    };
    if let Some(bio) = &profile_data.bio {
        validate_bio(bio)?;
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

pub fn clean_profile_create_data(
    profile_data: &mut ProfileCreateData,
) -> Result<(), ValidationError> {
    let is_remote = profile_data.actor_json.is_some();
    if let Some(ref display_name) = profile_data.display_name {
        let clean_name = clean_display_name(display_name, is_remote);
        profile_data.display_name = Some(clean_name);
    };
    if let Some(bio) = &profile_data.bio {
        let clean_bio = clean_bio(bio, is_remote);
        profile_data.bio = Some(clean_bio);
    };
    validate_profile_create_data(profile_data)?;
    Ok(())
}

fn validate_profile_update_data(
    profile_data: &ProfileUpdateData,
) -> Result<(), ValidationError> {
    validate_username(&profile_data.username)?;
    if let WebfingerHostname::Remote(ref hostname) = profile_data.hostname {
        validate_hostname(hostname)?;
    };
    if let WebfingerHostname::Unknown = profile_data.hostname {
        return Err(ValidationError("unknown hostname"));
    };
    if let Some(display_name) = &profile_data.display_name {
        validate_display_name(display_name)?;
    };
    let is_remote = if let Some(ref actor) = profile_data.actor_json {
        validate_actor_data(actor)?;
        if !actor.is_portable() && profile_data.hostname.as_str().is_none() {
            return Err(ValidationError(
                "non-portable remote profile should have hostname"));
        };
        true
    } else {
        false
    };
    if let Some(bio) = &profile_data.bio {
        validate_bio(bio)?;
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
    let is_remote = profile_data.actor_json.is_some();
    if let Some(ref display_name) = profile_data.display_name {
        let clean_name = clean_display_name(display_name, is_remote);
        profile_data.display_name = Some(clean_name);
    };
    if let Some(bio) = &profile_data.bio {
        let clean_bio = clean_bio(bio, is_remote);
        profile_data.bio = Some(clean_bio);
    };
    validate_profile_update_data(profile_data)?;
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
    fn test_clean_display_name() {
        let name = "test <script>alert()</script>test :emoji:";
        let output = clean_display_name(name, true);
        assert_eq!(output, "test test :emoji:");
    }

    #[test]
    fn test_clean_display_name_whitespace() {
        let name = "ワフ   ⁰͡ ⌵ ⁰͡ ";
        let output = clean_display_name(name, true);
        assert_eq!(output, "ワフ   ⁰͡ ⌵ ⁰͡ ");
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
        let result = clean_bio(bio, true);
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
            hostname: WebfingerHostname::Remote("social.example".to_string()),
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
