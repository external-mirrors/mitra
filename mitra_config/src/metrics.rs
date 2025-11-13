use serde::Deserialize;

#[derive(Clone, Deserialize)]
pub struct Metrics {
    pub auth_username: String,
    pub auth_password: String,
}
