//! The imperative shell: config discovery, the composition root, and HTTP.
//!
//! This crate is the only one allowed to see every other crate, because it is
//! the only one that *wires* anything (workshop 001 rule 4).

pub mod answer;
pub mod config;
pub mod enrich;
pub mod http;
pub mod roots;
pub mod runner;
pub mod scan;
pub mod schema;
pub mod search;
pub mod status;
pub mod wiring;

pub use answer::{Answer, IntoFailure};
pub use config::{
    ConfigError, SecretsLoaded, config_dir, load_config, load_effective_from, load_secrets,
};
pub use http::{router, serve};
pub use runner::{drain, run_forever};
pub use wiring::AppState;
