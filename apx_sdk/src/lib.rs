//! # APx
//!
//! A minimalistic [ActivityPub](https://www.w3.org/TR/activitypub/) toolkit written in Rust.
//!
//! Features:
//!
//! - Networking.
//! - Authentication (HTTP signatures, object integrity proofs).
//! - WebFinger.
//! - Nomadic identity (client & server).
//! - WebAssembly support.
//!
//! Using in a Cargo project:
//!
//! ```toml
//! [dependencies]
//! apx_sdk = "0.22.0"
//! ```
//!
//! ## Examples
//!
//! Fetch an object:
//!
//! ```rust
//! use apx_sdk::{
//!     agent::FederationAgent,
//!     fetch::{fetch_object, FetchObjectOptions},
//! };
//! # use apx_sdk::fetch::FetchError;
//! # async fn run() -> Result<(), FetchError> {
//! let object_id = "https://mastodon.social/@Mastodon";
//! let agent = FederationAgent::default();
//! let options = FetchObjectOptions::default();
//! let object = fetch_object(&agent, object_id, options).await?;
//! # Ok(())
//! # }
//! ```
//!
//! More examples:
//!
//! - [FEP-ae97 client](https://codeberg.org/silverpill/mitra/src/branch/main/apx_sdk/examples/fep_ae97_client.rs)
//! - [FEP-ae97 server](https://codeberg.org/silverpill/mitra/src/branch/main/apx_sdk/examples/fep_ae97_server.rs)

pub mod addresses;
pub mod agent;
pub mod authentication;
pub mod constants;
pub mod deliver;
pub mod deserialization;
pub mod fetch;
mod http_client;
pub mod http_server;
pub mod identifiers;
pub mod jrd;
pub mod ownership;
pub mod utils;

pub use apx_core as core;
