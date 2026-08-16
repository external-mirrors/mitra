use apx_core::url::common::Uri;

use super::errors::ValidationError;

// https://docs.joinmastodon.org/api/oauth-scopes/
const ALLOWED_SCOPES: [&str; 3] = ["read", "write", "profile"];

pub fn validate_redirect_uri(uri: &str) -> Result<(), ValidationError> {
    // https://www.rfc-editor.org/rfc/rfc6749#appendix-A.6
    Uri::try_from(uri)
        .map_err(|_| ValidationError("invalid redirect URI"))?;
    Ok(())
}

fn split_scopes(scopes: &str) -> Vec<String> {
    scopes.split_whitespace()
        .map(|scope| scope.to_owned())
        .collect()
}

pub fn clean_scopes(scopes: &str) -> (Vec<String>, Vec<String>) {
    let mut scopes = split_scopes(scopes);
    scopes.sort();
    scopes.dedup();
    let mut supported = vec![];
    let mut unsupported = vec![];
    for scope in scopes {
        if ALLOWED_SCOPES.contains(&scope.as_str()) {
            supported.push(scope);
        } else {
            unsupported.push(scope);
        };
    };
    (supported, unsupported)
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

    #[test]
    fn test_split_scopes() {
        let scopes = "read write push";
        assert_eq!(split_scopes(scopes), vec!["read", "write", "push"]);
    }

    #[test]
    fn test_clean_scopes() {
        let scopes = "read read:blocks write push";
        let (supported, unsupported) = clean_scopes(scopes);
        assert_eq!(supported, vec!["read", "write"]);
        assert_eq!(unsupported, vec!["push", "read:blocks"]);
    }

    #[test]
    fn test_clean_scopes_ordering() {
        let scopes = "write read";
        let (supported, unsupported) = clean_scopes(scopes);
        assert_eq!(supported, vec!["read", "write"]);
        assert_eq!(unsupported.is_empty(), true);
    }

    #[test]
    fn test_clean_scopes_with_duplicates() {
        let scopes = "read read read:blocks";
        let (supported, unsupported) = clean_scopes(scopes);
        assert_eq!(supported, vec!["read"]);
        assert_eq!(unsupported, vec!["read:blocks"]);
    }

    #[test]
    fn test_clean_scopes_empty_string() {
        let scopes = "";
        let (supported, unsupported) = clean_scopes(scopes);
        assert_eq!(supported.is_empty(), true);
        assert_eq!(unsupported.is_empty(), true);
    }
}
