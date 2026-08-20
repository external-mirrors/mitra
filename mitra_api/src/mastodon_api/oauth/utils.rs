use apx_core::base64;
use serde::Serialize;

use mitra_utils::{
    random::generate_random_sequence,
};

use crate::templates::render_template;

// Should be less than 10 minutes
// https://www.rfc-editor.org/rfc/rfc6749#section-4.1.2
pub(super) const AUTHORIZATION_CODE_LIFETIME: i64 = 60 * 5;

const NONCE_SIZE: usize = 10;

#[derive(Serialize)]
struct AuthorizationPage {
    nonce: String,
    code: Option<String>,
}

fn generate_nonce() -> String {
    let value: [u8; NONCE_SIZE] = generate_random_sequence();
    hex::encode(value)
}

pub fn render_authorization_page() -> (String, String) {
    let nonce = generate_nonce();
    let context = AuthorizationPage {
        nonce: nonce.clone(),
        code: None,
    };
    let html = render_template(
        include_str!("templates/base.html"),
        context,
    ).expect("template should be valid");
    (html, nonce)
}

pub fn render_authorization_code_page(code: String) -> (String, String) {
    let nonce = generate_nonce();
    let context = AuthorizationPage {
        nonce: nonce.clone(),
        code: Some(code),
    };
    let html = render_template(
        include_str!("templates/base.html"),
        context,
    ).expect("template should be valid");
    (html, nonce)
}

const ACCESS_TOKEN_SIZE: usize = 20;

fn encode_token(value: [u8; ACCESS_TOKEN_SIZE]) -> String {
    base64::encode_urlsafe_no_pad(value)
}

pub fn generate_oauth_token() -> String {
    let value: [u8; ACCESS_TOKEN_SIZE] = generate_random_sequence();
    encode_token(value)
}

fn is_subscope(scope_1: &str, scope_2: &str) -> bool {
    let scope_2_seq: Vec<_> = scope_2.split(":").collect();
    let scope_1_seq: Vec<_> = scope_1.split(":").take(scope_2_seq.len()).collect();
    scope_2_seq == scope_1_seq
}

pub fn verify_scopes(
    token_scopes: &[String],
    app_scopes: &[String],
) -> bool {
    for token_scope in token_scopes {
        if !app_scopes.iter().any(|app_scope| is_subscope(token_scope, app_scope)) {
            return false;
        };
    };
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_token() {
        let value = [87, 31, 60, 176, 41, 131, 140, 213, 30, 64, 78, 169, 144, 138, 61, 62, 127, 26, 140, 96];
        let token = encode_token(value);
        assert_eq!(token, "Vx88sCmDjNUeQE6pkIo9Pn8ajGA");
    }

    #[test]
    fn test_is_subscope() {
        assert_eq!(is_subscope("read", "read"), true);
        assert_eq!(is_subscope("read", "write"), false);
        assert_eq!(is_subscope("admin:read:accounts", "admin"), true);
        assert_eq!(is_subscope("admin:read:accounts", "admin:read"), true);
        assert_eq!(is_subscope("admin:read", "admin:read:accounts"), false);
    }

    #[test]
    fn test_verify_scopes() {
        let app_scopes = vec!["read".to_owned(), "write".to_owned()];

        let token_scopes_1 = vec!["read".to_owned()];
        assert_eq!(verify_scopes(&token_scopes_1, &app_scopes), true);
        let token_scopes_2 = vec!["admin".to_owned()];
        assert_eq!(verify_scopes(&token_scopes_2, &app_scopes), false);
    }
}
