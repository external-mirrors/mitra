mod authentication;
mod blockchain;
mod config;
mod environment;
mod federation;
mod instance;
mod limits;
mod loader;
mod metrics;
mod registration;
mod retention;

pub use authentication::AuthenticationMethod;
pub use blockchain::{
    BlockchainConfig,
    MoneroConfig,
};
pub use config::Config;
pub use environment::Environment;
pub use instance::Instance;
pub use limits::{Limits, MediaLimits, PostLimits};
pub use loader::parse_config;
pub use registration::{DefaultRole, RegistrationType};

pub const SOFTWARE_NAME: &str = "Mitra";
pub const SOFTWARE_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const SOFTWARE_REPOSITORY: &str = "https://codeberg.org/silverpill/mitra";

#[derive(thiserror::Error, Debug)]
#[error("{0}")]
pub struct ConfigError(&'static str);
