//! The `flowspace3` CLI: a thin HTTP client of the daemon (PRD req 33).
//!
//! It knows two things — where the daemon is, and how to ask it. It never
//! touches Postgres, never parses source, and never starts infrastructure
//! (PRD req 37: fail fast, `flowspace3 doctor` heals).

pub mod client;
pub mod settings;

pub use client::{DaemonClient, HealthReport};
pub use settings::daemon_url;

/// The suggestion every unreachable-daemon failure ends with (PRD req 37).
pub const DOCTOR_HINT: &str = "run `flowspace3 doctor` to diagnose and start the stack";
