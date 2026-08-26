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

    /// Two tree snapshots that describe different repositories were diffed.
    #[error("snapshot mismatch: {old} is not {new}")]
    SnapshotMismatch { old: String, new: String },
}

/// Core's result alias. Ports return this so `dyn` seams stay adapter-agnostic.
pub type Result<T> = std::result::Result<T, Error>;
