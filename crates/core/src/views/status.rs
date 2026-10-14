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
    /// Daemon-computed native conversation polling health.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversations: Option<ConversationsStatus>,
    /// The most recent failure that no later success of the same job kind and
    /// key has superseded, when there is one — so a status line can say what
    /// went wrong rather than only that something did.
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
    /// The embedder has stopped answering, from the first failure until the
    /// next answer. Absent while it answers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedder_failing: Option<EmbedderFailing>,
    /// How long recent searches took, by stage. Absent before the first
    /// search since the daemon started.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search_latency: Option<SearchLatency>,
}

/// Percentiles over the most recent searches this daemon answered.
///
/// Kept in memory, so it starts empty at every restart: a regression is
/// visible here without running a bench, and the bench owns the long view.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchLatency {
    /// Searches the percentiles are taken over (the most recent, bounded).
    pub searches: usize,
    /// Of those, how many took longer than `slow_ms`.
    pub slow: usize,
    /// The threshold `slow` counts against.
    pub slow_ms: u64,
    /// The stage with the highest p95, total and store excluded: where a slow
    /// search's time usually goes.
    pub slowest_stage: String,
    /// One row per stage, `total` first.
    pub stages: Vec<StageLatency>,
}

/// One stage's latency over [`SearchLatency::searches`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageLatency {
    /// The stage, named as in a search's `meta.timing` without `_ms`.
    pub stage: String,
    /// Median, in milliseconds.
    pub p50_ms: u64,
    /// 95th percentile, in milliseconds.
    pub p95_ms: u64,
    /// Slowest, in milliseconds.
    pub max_ms: u64,
}

/// The embedder has failed every call since `since` (DL-027).
///
/// Searches still answer from keywords meanwhile, and say so; this is where a
/// reader learns how long that has been going on and why.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmbedderFailing {
    /// UTC time of the first failure with no answer since.
    pub since: String,
    /// The most recent failure, as the provider or the time bound reported it.
    pub last_error: String,
    /// Calls that have failed since `since`.
    pub failures: u64,
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

/// The daemon owns this verdict; clients render it rather than re-deriving it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConversationState {
    Flowing,
    Stalled,
    Disabled,
}

impl ConversationState {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Flowing => "flowing",
            Self::Stalled => "stalled",
            Self::Disabled => "disabled",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationsStatus {
    pub state: ConversationState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_reason: Option<String>,
    pub last_poll_at: Option<String>,
    pub harnesses: Vec<ConversationHarnessStatus>,
    /// Stored conversations the intake restatement (migration 0030) has still
    /// to reach. Zero, and absent from the envelope, once it is done.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub restate_pending: i64,
}

#[allow(clippy::trivially_copy_pass_by_ref)] // serde's skip predicate takes a reference
fn is_zero(count: &i64) -> bool {
    *count == 0
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationHarnessStatus {
    pub harness: String,
    /// Eligible parent sessions, each including its sidecar files.
    pub tracked: usize,
    /// Parent sessions needing an append, truncation, or replacement read.
    pub behind: usize,
    /// Parent sessions with an unchanged file whose last terminal read failed.
    /// These files wait for a revision change and do not contribute to behind.
    #[serde(default)]
    pub unreadable: usize,
    /// The session ids behind `unreadable`, so the count can be acted on
    /// without a query: pair one with the failed `ingest:` job that names it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unreadable_sessions: Vec<String>,
    /// Time of the newest observed ingest report; unknown after daemon restart.
    pub newest_ingest_at: Option<String>,
    /// Actual report counters, never inferred from cursor movement.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub newest_ingest: Option<ConversationIngestReceipt>,
}

/// Totals projected from the existing ingest report for one request, including
/// any sidecars. `summarized` counts queued summaries, not completed calls or money.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationIngestReceipt {
    pub at: String,
    pub address: String,
    pub records_read: usize,
    pub turns_new: usize,
    pub deduped: usize,
    pub summarized: usize,
    pub rescanned: bool,
    pub contended: usize,
}

impl ConversationIngestReceipt {
    /// A counter line from this report, shared by human diagnostic surfaces.
    #[must_use]
    pub fn describe(&self) -> String {
        format!(
            "{} {} · read {} · new {} · deduped {} · summarized {} · rescanned {} · contended {}",
            self.at,
            self.address,
            self.records_read,
            self.turns_new,
            self.deduped,
            self.summarized,
            self.rescanned,
            self.contended
        )
    }
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
    /// Whether dot-prefixed directories are enabled for this root.
    #[serde(default)]
    pub include_hidden: bool,
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

/// The most recent failed job that nothing has fixed since.
///
/// A failure followed by a successful job of the same kind and key is not
/// reported at all: it is history, and showing it here read as a live bug.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LastError {
    /// Which job — the dedupe key names the file or the content.
    pub job: String,
    /// What it said.
    pub error: String,
    /// Row id of the failed job. Absent only from a daemon that predates it.
    #[serde(default)]
    pub job_id: Option<i64>,
    /// When it failed, UTC. Absent only from a daemon that predates it.
    #[serde(default)]
    pub at: Option<String>,
    /// Seconds from the failure to this report, so a reader sees how stale it is
    /// without a clock. Absent only from a daemon that predates it.
    #[serde(default)]
    pub age_secs: Option<i64>,
}
