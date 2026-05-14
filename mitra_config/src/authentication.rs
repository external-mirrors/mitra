use serde::{
    Deserialize,
    Deserializer,
    de::Error as DeserializerError,
};

#[derive(Clone, PartialEq)]
pub enum AuthenticationMethod {
    Password,
    Eip4361,
    Caip122Monero,
}

impl<'de> Deserialize<'de> for AuthenticationMethod {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where D: Deserializer<'de>
    {
        let method_str = String::deserialize(deserializer)?;
        let method = match method_str.as_str() {
            "password" => Self::Password,
            "eip4361" => Self::Eip4361,
            "caip122_monero" => Self::Caip122Monero,
            _ => return Err(DeserializerError::custom("unknown authentication method")),
        };
        Ok(method)
    }
}

pub fn default_authentication_methods() -> Vec<AuthenticationMethod> {
    vec![AuthenticationMethod::Password]
}

pub fn default_authentication_token_lifetime() -> u32 { 86400 * 30 }

pub fn default_login_message() -> String { "Do not sign this message on other sites!".to_string() }
