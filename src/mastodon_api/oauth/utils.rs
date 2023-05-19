use base64;

use mitra_utils::random::generate_random_sequence;

pub fn render_authorization_page() -> String {
    format!(include_str!("template.html"), content=r#"
    <form method="post">
        <input type="text" name="username" placeholder="Username">
        <br>
        <input type="password" name="password" placeholder="Password">
        <br>
        <button type="submit">Submit</button>
    </form>"#)
}

pub fn render_authorization_code_page(code: String) -> String {
    format!(include_str!("template.html"), content=code)
}

const ACCESS_TOKEN_SIZE: usize = 20;

pub fn generate_access_token() -> String {
    let value: [u8; ACCESS_TOKEN_SIZE] = generate_random_sequence();
    base64::encode_config(value, base64::URL_SAFE_NO_PAD)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_access_token() {
        let token = generate_access_token();
        assert!(token.len() > ACCESS_TOKEN_SIZE);
    }
}
