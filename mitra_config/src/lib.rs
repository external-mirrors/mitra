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
mod software;

pub use authentication::AuthenticationMethod;
pub use blockchain::{
    BlockchainConfig,
    MoneroConfig,
    MoneroLightConfig,
};
pub use config::Config;
pub use environment::Environment;
pub use instance::Instance;
pub use limits::{Limits, MediaLimits, PostLimits};
pub use loader::parse_config;
pub use registration::{DefaultRole, RegistrationType};
pub use software::SoftwareMetadata;

#[derive(thiserror::Error, Debug)]
#[error("{0}")]
pub struct ConfigError(&'static str);
