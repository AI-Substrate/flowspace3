//! Readers for the native agent-session stores — implementations of the
//! [`ConversationSource`] port (plan 005).
//!
//! One module per store, and that is the whole convergence protocol: a reader
//! owns its file and adds exactly ONE line to this module, so four readers
//! written in four worktrees converge on a trivial merge instead of a
//! conflict. Nothing here knows about any other reader.
//!
//! Shared, because it is framing rather than dialect: [`tail::read_lines`],
//! the incremental line reader every jsonl store resumes through. Dialects —
//! claude's per-block merge, omp's `xd://` remap, copilot's `type`-not-`name`
//! event naming — stay inside their own module. A helper that starts to know
//! which store called it belongs in that store's module instead.
//!
//! [`ConversationSource`]: fs3_core::ConversationSource

pub mod claude;
pub mod metrics_db;
pub mod omp;
pub mod pij_ledger;
pub mod tail;

// One `pub mod` line per reader lands here. Keep them alphabetical.
