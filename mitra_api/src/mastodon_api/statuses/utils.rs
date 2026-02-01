use mitra_utils::languages::Language;
use mitra_validators::errors::ValidationError;

pub fn parse_language_code(value: &str) -> Result<Language, ValidationError> {
    Language::from_639_1(value)
        .ok_or(ValidationError("invalid ISO 639-1 code"))
}
