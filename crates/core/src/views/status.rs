//! What `status` answers with.

use serde::{Deserialize, Serialize};

/// What `GET /status` answers with.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusReport {
    /// Every registered worktree, with its file count.
    pub roots: Vec<Root>,
    /// The live queue, grouped by kind and state.
    pub queue: Vec<QueueRow>,
    /// The daemon-owned done-job retention receipt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retention: Option<RetentionStatus>,
    /// The most recent failure, when there is one — so a status line can say
    /// what went wrong rather than only that something did.
    pub last_error: Option<LastError>,
    /// Dirty element-tree shapes found without failing this read.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inconsistencies: Vec<ElementTreeInconsistency>,
    /// Migrations the DATABASE has that this binary does not.
    ///
    /// Empty in the normal case. Non-empty means a newer daemon has migrated
    /// this database, which is worth saying out loud: it explains a column
    /// nobody here expects, and it is the first thing to check when two daemons
    /// disagree.
    pub schema_ahead: Vec<i64>,
}

/// The most recently completed done-job retention sweep.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetentionStatus {
    /// Configured age window before a completed job becomes purgeable.
    pub window_days: u32,
    /// UTC timestamp of the last completed sweep, absent before the first pass.
    pub last_purge_at: Option<String>,
    /// Number of rows removed by that complete sweep.
    pub purged_last_run: u64,
}

/// One shared blob whose parsed rows do not form exactly one file tree.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ElementTreeInconsistency {
    /// Content-addressed file bytes affected by the inconsistency.
    pub blob_sha: String,
    /// Parser key whose rows are dirty.
    pub parser_version: String,
    /// Every stored root path, in deterministic survivor order.
    pub paths: Vec<String>,
    /// Concrete operator action that repairs the rows.
    pub next_action: String,
}

/// One registered root.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Root {
    /// The repository identity (PRD req 35).
    pub identity: String,
    /// Absolute host path of the added root.
    pub root_path: String,
    /// How many files fs3 currently maps for it.
    pub files: i64,
}

/// One `(kind, state)` bucket of the queue.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueueRow {
    /// `scan_file`, `summarize`, `embed`.
    pub kind: String,
    /// `pending`, `running`, `done`, `failed`.
    pub state: String,
    /// How many rows.
    pub count: i64,
    /// How many of them carry a `last_error` — a job that succeeded on its
    /// third attempt still counts, which is the difference between "flaky" and
    /// "fine".
    pub with_error: i64,
}

/// The most recent failed job.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LastError {
    /// Which job — the dedupe key names the file or the content.
    pub job: String,
    /// What it said.
    pub error: String,
}
