//! The imperative shell: config discovery, the composition root, and HTTP.
//!
//! This crate is the only one allowed to see every other crate, because it is
//! the only one that *wires* anything (workshop 001 rule 4).

pub mod answer;
pub mod auth;
pub mod batch;
pub mod boot;
pub mod config;
pub mod conversations;
pub mod debounce;
pub mod enrich;
pub mod gc;
pub mod http;
pub mod logging;
pub mod read;
pub mod reconcile;
pub mod remove;
pub mod roots;
pub mod runner;
pub mod scan;
pub mod schema;
pub mod scope;
pub mod search;
pub mod skew;
pub mod status;
pub mod update;
pub mod watch;
pub mod wiring;

pub use answer::{Answer, IntoFailure};
pub use auth::Auth;
pub use boot::run;
pub use config::{
    ConfigError, SecretsLoaded, config_dir, load_config, load_effective_from, load_secrets,
};
pub use gc::GcSupervisor;
pub use http::{router, serve};
pub use logging::{Logging, Roller, RollingWriter};
pub use reconcile::{Pass, Reconcile};
pub use runner::{drain, run_forever};
pub use skew::SchemaSupervisor;
pub use update::{Outcome, UpdateSupervisor, Updater};
pub use watch::WatcherSupervisor;
pub use wiring::AppState;
