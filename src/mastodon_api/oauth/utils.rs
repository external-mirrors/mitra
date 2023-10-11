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

fn encode_token(value: [u8; ACCESS_TOKEN_SIZE]) -> String {
    base64::encode_config(value, base64::URL_SAFE_NO_PAD)
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
