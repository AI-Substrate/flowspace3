//! The one error type core hands out. Adapters map their own failures into it.

/// Every fallible operation in core — and every port implementation — reports
/// through this type, so callers never depend on an adapter's error crate.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    /// A blob reference that is not a plausible content hash.
    #[error("invalid blob reference {value:?}: {reason}")]
    InvalidBlobRef { value: String, reason: &'static str },

    /// Configuration that parsed as TOML but does not describe a usable system.
    #[error("invalid config: {0}")]
    InvalidConfig(String),

    /// A port implementation failed. The string is the adapter's own message.
    #[error("provider failure: {0}")]
    Provider(String),

    /// A provider refused the work because we are asking too fast, and kept
    /// refusing after the adapter had retried its own way out.
    ///
    /// Distinct from [`Error::Provider`] because a scheduler can *act* on this
    /// one: `retry_after` is how long the service asked us to wait, so a lane
    /// can park the claim for exactly that long instead of guessing, or
    /// treating a temporary squeeze as a failed unit of work. A formatted
    /// string could carry the same information and no caller could use it.
    ///
    /// `retry_after` is `None` when the service rate-limited us without saying
    /// for how long — common, and the reason this is an `Option` rather than a
    /// number someone invented.
    #[error(
        "{provider} rate limited after {attempts} attempt(s){}",
        match retry_after {
            Some(wait) => format!("; retry after {}s", wait.as_secs()),
            None => String::new(),
        }
    )]
    RateLimited {
        /// Which provider said no, named the way its errors name it.
        provider: String,
        /// What the service asked us to wait, when it said.
        retry_after: Option<std::time::Duration>,
        /// How many times the adapter tried before giving the caller this.
        attempts: usize,
    },

    /// Two tree snapshots that describe different repositories were diffed.
    #[error("snapshot mismatch: {old} is not {new}")]
    SnapshotMismatch { old: String, new: String },
}

/// Core's result alias. Ports return this so `dyn` seams stay adapter-agnostic.
pub type Result<T> = std::result::Result<T, Error>;
