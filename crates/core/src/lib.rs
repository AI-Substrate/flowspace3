//! fs3's functional core: domain types, pure logic, and the only two ports.
//!
//! Workshop 001 rule 2 — *functional core, imperative shell*. Nothing in this
//! crate performs IO: no tokio, no sqlx, no HTTP client. Effects live at the
//! edges (`store`, `providers`, `daemon`). Consequently core's tests need
//! **zero doubles**.
//!
//! Workshop 001 rule 3 — a trait earns its existence only when a second real
//! implementation exists or is firmly planned. fs3 has FOUR ports:
//! [`Embedder`], [`Summarizer`], [`ChatProvider`] and [`ConversationSource`].
//! The last two were asked for and granted on the same day, 2026-08-28, by two
//! plans that did not know about each other: `ask` needs a CHAT model, which is
//! a different model from the one that summarises in bulk, and plan 005's
//! readers ship four real implementations of a session store on day one. A
//! FIFTH is stop-and-ask.

pub mod address;
pub mod agent;
pub mod catalog;
pub mod classify;
pub mod config;
pub mod conversation;
pub mod conversation_join;
pub mod conversation_normalize;
pub mod conversation_source;
pub mod ddoc;
pub mod ddoc_envelope;
pub mod element;
pub mod envelope;
pub mod error;
pub mod events;
pub mod git;
pub mod logging;
pub mod messages;
pub mod output;
pub mod ports;
pub mod skew;
pub mod tokens;
pub mod update;
pub mod views;

pub use address::{
    Address, AddressError, ConversationAddress, ElementAddress, ElementParts, element_address,
    element_path,
};
pub use agent::{
    AgentAnswer, AgentBounds, SYSTEM_PROMPT, StopReason, ToolBox, ToolOutcome, TraceEntry, ask,
};
pub use catalog::{Area, Code};
pub use classify::{category_hint, classify, is_declaration_shaped};
pub use config::{
    CONFIG_DIR_ENV, CONFIG_FILE_NAME, Config, DAEMON_KEY_FILE_NAME, DEFAULT_CONFIG_SUBDIR,
    DEFAULT_PROVIDER, DaemonConfig, DatabaseConfig, ENV_NESTING, ENV_PREFIX, Effective,
    IndexingConfig, Layer, Origin, Port, PortSelection, Problem, ProviderInstance, REDACTED,
    RepoSelection, SECRETS_FILE_NAME, SECTIONS, ScanConfig, Sources, UpdateConfig, daemon_key_path,
    env_overrides, parse_env_file, redact_url_password, resolve, resolve_config_dir,
};
pub use conversation::{
    Conversation, ConversationId, ToolInput, Turn, TurnItem, TurnRole, TurnSource, earns_summary,
};
pub use conversation_join::{
    SeatBinding, SessionRow, parse_rows, resolve_seat, store_for, uuid_version,
};
pub use conversation_normalize::{
    OUTPUT_HEAD_BYTES, PreparedBatch, normalize_record, prepare_batch, shape_turn,
};
pub use conversation_source::{
    ConversationSource, Harness, IngestInput, RawRecord, ReadBatch, SessionFile, SessionKind,
    SourceCursor,
};
pub use ddoc::{
    DDOC_ADDRESS_SEPARATOR, DDOC_GENERATED_BANNER, DDOC_GENERATED_SUFFIX, DDOC_SOURCE_SUFFIX,
    DdocAddress, DdocAddressError, DdocMeta, DdocRel, DdocSchemaFacts, DerivedState, EmbedBasis,
    default_gate_terminal, derive_state, minted_prefix,
};
pub use element::{
    ADDRESS_SEGMENT, BlobRef, Element, ElementKind, ElementTree, PreOrder, Span, content_hash,
    needs_summary,
};
pub use envelope::{ENVELOPE_VERSION, Envelope, Failure};
pub use error::{Error, Result};
pub use events::{Event, EventKind, HEARTBEAT_MS, Hello, STREAM_VERSION};
pub use git::{BlobChange, ChangedSet, FileBlob, IdentitySource, RepoIdentity, TreeSnapshot, diff};
pub use logging::{
    LOG_FILE_NAME, LOGGING_SOURCE, resolve_log_dir, rolled_name, unwritable_message,
};
pub use messages::{Severity, UserMessage};
pub use output::{OUTPUT_AUTO, OUTPUT_ENV, OutputMode};
pub use ports::{
    ChatMessage, ChatProvider, ChatTurn, Embedder, Summarizer, Summary, ToolCall, ToolSchema,
};
pub use skew::{SCHEMA_SOURCE, SchemaSkew};
pub use tokens::{BYTES_PER_TOKEN, estimate_tokens, fit_to_cap};
pub use update::{UPDATE_SOURCE, UpdateState, Version, is_upgrade};
