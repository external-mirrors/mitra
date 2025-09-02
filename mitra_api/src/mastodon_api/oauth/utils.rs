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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_token() {
        let value = [87, 31, 60, 176, 41, 131, 140, 213, 30, 64, 78, 169, 144, 138, 61, 62, 127, 26, 140, 96];
        let token = encode_token(value);
        assert_eq!(token, "Vx88sCmDjNUeQE6pkIo9Pn8ajGA");
    }
}
