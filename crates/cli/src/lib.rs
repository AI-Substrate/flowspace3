//! The `flowspace3` CLI: a thin HTTP client of the daemon (PRD req 33).
//!
//! It knows two things — where the daemon is, and how to ask it. It never
//! touches Postgres, never parses source, and never starts infrastructure
//! (PRD req 37: fail fast, `flowspace3 doctor` heals).

pub mod client;
pub mod docs;
pub mod doctor;
pub mod settings;
pub mod show;

pub use client::{DaemonClient, HealthReport};
pub use docs::{TopicList, TopicPage, TopicSummary};
pub use doctor::{DoctorReport, Step};
pub use settings::{config_dir, daemon_url, load_effective_from, load_secrets_from};

/// The suggestion every unreachable-daemon failure ends with (PRD req 37).
pub const DOCTOR_HINT: &str = "run `flowspace3 doctor` to diagnose and start the stack";
