//! Per-verb payload DTOs — the `data` half of the envelope, typed.
//!
//! # Why these live in core
//!
//! `Envelope` is generic in its payload precisely so the daemon can serialise a
//! typed `data` while a consumer deserialises an opaque `Value`. That works
//! until something wants to READ a payload with types — a human renderer, a
//! TUI, a future web client — and finds `SearchResults` inside `fs3-daemon`
//! (axum, sqlx, tokio) and `DoctorReport` inside `fs3-cli`. A consumer would
//! then have to depend on a web server to draw a table, so it does the only
//! other thing available: it mirrors the structs by hand, field for field, and
//! the two copies drift. `pocs/human-render/src/views.rs` is that mirror, and
//! `pocs/human-render/LEARNINGS.md` §3.1 calls the move done here "the one real
//! blocker" of the promotion.
//!
//! So the payloads sit beside the envelope they travel in. The producer and
//! every consumer now read the SAME definition, and a field added for one is a
//! field the other cannot miss. The functional-core rule is untouched: these
//! are plain serde structs with no IO, no pool, and no runtime.
//!
//! # What changed on the wire when they moved: nothing
//!
//! Field order is declaration order and every `#[serde]` attribute came across
//! verbatim, so serialisation is byte-identical — asserted by
//! `crates/cli/tests/envelope_goldens.rs`, whose goldens were captured from the
//! commit BEFORE this move. `Deserialize` was added where a producer-only type
//! had `Serialize` alone; deriving it changes no bytes and is what makes these
//! types readable by a consumer at all.

pub mod doctor;
pub mod read;
pub mod remove;
pub mod roots;
pub mod search;
pub mod status;
