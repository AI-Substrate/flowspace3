//! What `remove` and `gc` answer with.

use serde::{Deserialize, Serialize};

/// What `POST /remove` answers with.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoveReport {
    /// The absolute root path that was asked about.
    pub root_path: String,
    /// Whether anything was registered there. `false` is a successful answer,
    /// not a failure: `remove` on an unknown path is a question with a true
    /// answer.
    pub was_registered: bool,
    /// The repository identity it belonged to, when it was registered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<String>,
    /// Path→blob mappings that went with it.
    pub files: i64,
    /// Queued scans killed. Enrichment jobs are keyed by content rather than by
    /// root and are deliberately NOT counted here, because they are not
    /// touched.
    pub jobs_killed: i64,
    /// Whether the repo row went too, because no other checkout of it remained.
    pub repo_removed: bool,
    /// Rows GC could reclaim right now — a FLOOR, not a forecast, and not a
    /// promise of when.
    pub reclaimable: GcCounts,
    /// The roots that ARE registered, when the caller's path was not one of
    /// them. Empty otherwise.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub registered: Vec<String>,
}

/// What `POST /gc` answers with, and the shape [`RemoveReport`] borrows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GcCounts {
    /// Enrichment queued for content nothing holds.
    pub jobs: i64,
    /// Parse trees for blobs no worktree maps.
    pub elements: i64,
    /// Summaries no remaining element carries.
    pub summaries: i64,
    /// Vectors whose source is gone.
    pub embeddings: i64,
    /// Every row above, for a one-line answer.
    pub total: i64,
}
