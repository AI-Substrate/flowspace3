//! The imperative shell: config discovery, the composition root, and HTTP.
//!
//! This crate is the only one allowed to see every other crate, because it is
//! the only one that *wires* anything (workshop 001 rule 4).

pub mod config;
pub mod http;
pub mod wiring;

pub use config::{
    ConfigError, SecretsLoaded, config_dir, load_config, load_effective_from, load_secrets,
};
pub use http::{router, serve};
pub use wiring::AppState;
