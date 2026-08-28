//! fs3's functional core: domain types, pure logic, and the only two ports.
//!
//! Workshop 001 rule 2 — *functional core, imperative shell*. Nothing in this
//! crate performs IO: no tokio, no sqlx, no HTTP client. Effects live at the
//! edges (`store`, `providers`, `daemon`). Consequently core's tests need
//! **zero doubles**.
//!
//! Workshop 001 rule 3 — a trait earns its existence only when a second real
//! implementation exists or is firmly planned. fs3 has three ports:
//! [`Embedder`], [`Summarizer`] and [`ConversationSource`] — the last ruled by
//! prime on 2026-08-28 (plan 005) and shipping four real implementations on
//! day one. A FOURTH is stop-and-ask.

pub mod address;
pub mod catalog;
pub mod classify;
pub mod config;
pub mod conversation;
pub mod conversation_source;
pub mod element;
pub mod envelope;
pub mod error;
pub mod git;
pub mod logging;
pub mod messages;
pub mod ports;
pub mod skew;
pub mod tokens;
pub mod update;

pub use address::{
    Address, AddressError, ConversationAddress, ElementAddress, ElementParts, element_address,
    element_path,
};
pub use catalog::{Area, Code};
pub use classify::{category_hint, classify, is_declaration_shaped};
pub use config::{
    CONFIG_DIR_ENV, CONFIG_FILE_NAME, Config, DEFAULT_CONFIG_SUBDIR, DEFAULT_PROVIDER,
    DaemonConfig, DatabaseConfig, ENV_NESTING, ENV_PREFIX, Effective, IndexingConfig, Layer,
    Origin, Port, PortSelection, Problem, ProviderInstance, REDACTED, RepoSelection,
    SECRETS_FILE_NAME, SECTIONS, ScanConfig, Sources, UpdateConfig, env_overrides, parse_env_file,
    redact_url_password, resolve, resolve_config_dir,
};
pub use conversation::{
    Conversation, ConversationId, ToolInput, Turn, TurnItem, TurnRole, TurnSource, earns_summary,
};
pub use conversation_source::{
    ConversationSource, Harness, IngestInput, RawRecord, ReadBatch, SessionFile, SessionKind,
    SourceCursor,
};
pub use element::{
    ADDRESS_SEGMENT, BlobRef, Element, ElementKind, ElementTree, PreOrder, Span, content_hash,
    needs_summary,
};
pub use envelope::{ENVELOPE_VERSION, Envelope, Failure};
pub use error::{Error, Result};
pub use git::{BlobChange, ChangedSet, FileBlob, IdentitySource, RepoIdentity, TreeSnapshot, diff};
pub use logging::{
    LOG_FILE_NAME, LOGGING_SOURCE, resolve_log_dir, rolled_name, unwritable_message,
};
pub use messages::{Severity, UserMessage};
pub use ports::{Embedder, Summarizer, Summary};
pub use skew::{SCHEMA_SOURCE, SchemaSkew};
pub use tokens::{BYTES_PER_TOKEN, estimate_tokens, fit_to_cap};
pub use update::{UPDATE_SOURCE, UpdateState, Version, is_upgrade};
