//! fs3's functional core: domain types, pure logic, and the only two ports.
//!
//! Workshop 001 rule 2 — *functional core, imperative shell*. Nothing in this
//! crate performs IO: no tokio, no sqlx, no HTTP client. Effects live at the
//! edges (`store`, `providers`, `daemon`). Consequently core's tests need
//! **zero doubles**.
//!
//! Workshop 001 rule 3 — a trait earns its existence only when a second real
//! implementation exists or is firmly planned. fs3 v1 has exactly two ports:
//! [`Embedder`] and [`Summarizer`]. A third is stop-and-ask.

pub mod classify;
pub mod config;
pub mod element;
pub mod error;
pub mod ports;

pub use classify::{category_hint, classify, is_declaration_shaped};
pub use config::{
    CONFIG_DIR_ENV, CONFIG_FILE_NAME, Config, DEFAULT_CONFIG_SUBDIR, DaemonConfig, DatabaseConfig,
    IndexingConfig, ProviderConfig,
};
pub use element::{BlobRef, Element, ElementKind, needs_summary};
pub use error::{Error, Result};
pub use ports::{Embedder, Summarizer, Summary};
