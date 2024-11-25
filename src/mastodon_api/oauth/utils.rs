use apx_core::base64;

use mitra_utils::{
    random::generate_random_sequence,
};

// Should be less than 10 minutes
// https://www.rfc-editor.org/rfc/rfc6749#section-4.1.2
pub(super) const AUTHORIZATION_CODE_LIFETIME: i64 = 60 * 5;

const NONCE_SIZE: usize = 10;

fn generate_nonce() -> String {
    let value: [u8; NONCE_SIZE] = generate_random_sequence();
    hex::encode(value)
}

pub fn render_authorization_page() -> (String, String) {
    let nonce = generate_nonce();
    let html = format!(
        include_str!("templates/base.html"),
        nonce=nonce,
        content=include_str!("templates/form.html"),
    );
    (html, nonce)
}

pub fn render_authorization_code_page(code: String) -> (String, String) {
    let nonce = generate_nonce();
    let html = format!(
        include_str!("templates/base.html"),
        nonce=nonce,
        content=code,
    );
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
