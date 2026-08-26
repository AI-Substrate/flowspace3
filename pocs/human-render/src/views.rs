//! The per-verb payload shapes, as the RENDERER sees them.
//!
//! # Why these are mirrored rather than imported
//!
//! The envelope itself lives in `fs3-core` and is imported. Its `data` payloads
//! do not: `SearchResults`/`Hit` live in `fs3-daemon` (which carries axum,
//! sqlx and tokio) and `DoctorReport`/`Step` live in `fs3-cli` (which carries
//! the whole client). A renderer that wanted the real types would have to
//! depend on a web server to draw a table.
//!
//! So the payloads are re-declared here, deserialize-only, matching the frozen
//! JSON field-for-field. That is a finding, not a shrug — see LEARNINGS.md:
//! promotion would move these DTOs into `fs3-core` beside the envelope, at
//! which point this module deletes itself.
//!
//! # Everything is `#[serde(default)]` on purpose
//!
//! `v` bumps only when the ENVELOPE breaks; a verb's `data` grows fields
//! additively by contract. A renderer that hard-failed on a missing or unknown
//! field would turn a forward-compatible payload into a blank screen. Missing
//! optional facts degrade to "not shown"; unknown fields are ignored. The only
//! thing the renderer refuses to invent is a value it was not given.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::Value;

/// `search` — the ranked rows (workshop 003's result envelope).
#[derive(Clone, Debug, Default, Deserialize)]
pub struct SearchResults {
    /// Best first. The renderer never re-sorts: rank is the daemon's answer.
    #[serde(default)]
    pub results: Vec<Hit>,
}

/// One hit.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Hit {
    /// `el:<repo>/<path>::<container>::<name>` — the only id surface (D7).
    #[serde(default)]
    pub address: String,
    /// 1.0 is identical.
    #[serde(default)]
    pub score: f64,
    /// Which vector space won: `raw` or `smart`.
    #[serde(default)]
    pub match_field: String,
    /// Universal category: `function`, `class`, `file`…
    #[serde(default)]
    pub kind: String,
    /// The grammar's own kind.
    #[serde(default)]
    pub subkind: String,
    /// The declaration's own name.
    #[serde(default)]
    pub name: String,
    /// Inclusive 1-based `[start, end]`.
    #[serde(default)]
    pub span: Option<[u32; 2]>,
    /// The first lines of the element's own text.
    #[serde(default)]
    pub snippet: String,
    /// The summary, when there is one.
    #[serde(default)]
    pub smart: Option<String>,
    /// Concept tags from the summary (PRD req 36).
    #[serde(default)]
    pub tags: Vec<String>,
    /// The repository a live path holding this content belongs to.
    #[serde(default)]
    pub repo: Option<String>,
    /// A live path holding it, relative to its worktree root.
    #[serde(default)]
    pub path: Option<String>,
}

/// The half of `meta` the search surface reads (workshop 003).
///
/// `meta` is "never load-bearing" by contract, so every field here is optional
/// and the header simply says less when the daemon said less.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct SearchMeta {
    /// Honest total before `limit`/`offset`.
    #[serde(default)]
    pub total: Option<u64>,
    /// The window this page represents.
    #[serde(default)]
    pub showing: Option<Showing>,
    /// `semantic`, `text`, …
    #[serde(default)]
    pub mode: Option<String>,
    /// `rrf` or `vector`.
    #[serde(default)]
    pub rank: Option<String>,
    /// The agent steer that is also a great human steer: which folders the
    /// answers came from, and how many from each.
    #[serde(default)]
    pub folders: BTreeMap<String, u64>,
    /// What actually narrowed the search — so a surprising result set is
    /// explainable without re-reading the command line.
    #[serde(default)]
    pub filters_applied: BTreeMap<String, Value>,
    /// Wall time, when the daemon reports it.
    #[serde(default)]
    pub took_ms: Option<u64>,
}

/// The `{from, count}` window.
#[derive(Clone, Copy, Debug, Default, Deserialize)]
pub struct Showing {
    /// Zero-based offset of the first row.
    #[serde(default)]
    pub from: u64,
    /// How many rows this page carries.
    #[serde(default)]
    pub count: u64,
}

/// `doctor` — the walk, one row per check.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct DoctorReport {
    /// Every step, in dependency order.
    #[serde(default)]
    pub steps: Vec<Step>,
    /// Whether the store is usable now.
    #[serde(default)]
    pub healthy: bool,
}

/// One step of the walk.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Step {
    /// `engine`, `stack`, `database`, `schema`…
    #[serde(default)]
    pub check: String,
    /// `ok`, `repaired`, or `failed`.
    #[serde(default)]
    pub outcome: String,
    /// What doctor found.
    #[serde(default)]
    pub found: String,
    /// What doctor did — absent when there was nothing to do.
    #[serde(default)]
    pub action: Option<String>,
    /// How long the step took.
    #[serde(default)]
    pub elapsed_ms: Option<u128>,
}

/// `status` — what is registered, and what is left to do.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct StatusReport {
    /// Every registered worktree, with its file count.
    #[serde(default)]
    pub roots: Vec<Root>,
    /// The queue, grouped by kind and state.
    #[serde(default)]
    pub queue: Vec<QueueRow>,
    /// The most recent failure, when there is one.
    #[serde(default)]
    pub last_error: Option<LastError>,
    /// Migrations the DATABASE has that this binary does not.
    #[serde(default)]
    pub schema_ahead: Vec<i64>,
}

/// One registered root.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Root {
    /// The repository identity (PRD req 35).
    #[serde(default)]
    pub identity: String,
    /// Absolute host path of the added root.
    #[serde(default)]
    pub root_path: String,
    /// How many files fs3 currently maps for it.
    #[serde(default)]
    pub files: i64,
}

/// One `(kind, state)` bucket of the queue.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct QueueRow {
    /// `scan_file`, `summarize`, `embed`.
    #[serde(default)]
    pub kind: String,
    /// `pending`, `running`, `done`, `failed`.
    #[serde(default)]
    pub state: String,
    /// How many rows.
    #[serde(default)]
    pub count: i64,
    /// How many of them carry a `last_error`.
    #[serde(default)]
    pub with_error: i64,
}

/// The most recent failed job.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct LastError {
    /// Which job — the dedupe key names the file or the content.
    #[serde(default)]
    pub job: String,
    /// What it said.
    #[serde(default)]
    pub error: String,
}
