use pbkdf2::{
    Pbkdf2,
    password_hash::{PasswordHash, PasswordVerifier},
};
use thiserror::Error;

use super::random::generate_random_sequence;

const PBKDF2_SHA512_PREFIX: &str = "$pbkdf2-sha512$";

#[derive(Debug, Error)]
pub enum PasswordError {
    #[error(transparent)]
    Argon2(#[from] argon2::Error),
    #[error(transparent)]
    Pbkdf2(#[from] pbkdf2::password_hash::Error),
}

pub fn hash_password(password: &str) -> Result<String, PasswordError> {
    let salt: [u8; 32] = generate_random_sequence();
    let config = argon2::Config::default();

    let digest = argon2::hash_encoded(password.as_bytes(), &salt, &config)?;
    Ok(digest)
}

pub fn verify_password(
    password_digest: &str,
    password: &str,
) -> Result<bool, PasswordError> {
    if let Some(body) = password_digest.strip_prefix(PBKDF2_SHA512_PREFIX) {
        // Pleroma stores password hashes as `$pbkdf2-sha512$ITERATIONS$SALT$HASH`,
        // where SALT and HASH use Elixir base64 (standard alphabet with `.`
        // instead of `+`, no padding).
        let (iterations, rest) = body.split_once('$').unwrap_or((body, ""));
        let hash_length = rest.rsplit('$').next().unwrap_or("").len();
        let output_length = hash_length * 3 / 4;
        let phc_digest = format!(
            "{PBKDF2_SHA512_PREFIX}i={iterations},l={output_length}${}",
            rest.replace('.', "+"),
        );
        let parsed_digest = PasswordHash::new(&phc_digest)?;
        let is_valid = Pbkdf2
            .verify_password(password.as_bytes(), &parsed_digest)
            .is_ok();
        return Ok(is_valid);
    };
    let is_valid = argon2::verify_encoded(
        password_digest,
        password.as_bytes(),
    )?;
    Ok(is_valid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verify_password() {
        let password = "$test123";
        let password_digest = hash_password(password).unwrap();
        let result = verify_password(&password_digest, password);
        assert_eq!(result.is_ok(), true);
    }

    #[test]
    fn test_verify_password_pbkdf2() {
        let password = "test-password";
        let password_digest = concat!(
            "$pbkdf2-sha512$1000$",
            "MDEyMzQ1Njc4OWFiY2RlZg$",
            "Miy.SnBodN4bdKyaapBEGjmNCyo6NwNId458/vQo0k5o",
            "lol/vm.nAA7yH0.wyiZUvXIyhdJyWMUeHIAmW5wDoQ",
        );
        let result = verify_password(password_digest, password)
            .unwrap();
        assert_eq!(result, true);
    }
}
