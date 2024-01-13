use mitra_utils::urls::Url;

use super::errors::ValidationError;

pub fn validate_redirect_uri(uri: &str) -> Result<(), ValidationError> {
    Url::parse(uri)
        .map_err(|_| ValidationError("invalid redirect URI"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_redirect_uri_scheme_https() {
        let redirect_uri = "https://app.example";
        assert!(validate_redirect_uri(redirect_uri).is_ok());
    }

    #[test]
    fn test_get_redirect_uri_scheme_app() {
        let redirect_uri = "fedilab://backtofedilab";
        assert!(validate_redirect_uri(redirect_uri).is_ok());
    }
}
