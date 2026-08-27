//! Removing a root, and reclaiming what nothing references (PRD req 57).
//!
//! The daemon half of the two verbs. Both are thin: the decisions and the SQL
//! live in [`fs3_store::roots`], and these shape the answer.

use fs3_core::catalog;
use fs3_core::envelope::Failure;
use serde::{Deserialize, Serialize};

use crate::answer::IntoFailure;
use crate::wiring::AppState;

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
    #[serde(skip_serializing_if = "Option::is_none")]
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

impl From<fs3_store::Reclaimed> for GcCounts {
    fn from(reclaimed: fs3_store::Reclaimed) -> Self {
        Self {
            jobs: reclaimed.jobs,
            elements: reclaimed.elements,
            summaries: reclaimed.summaries,
            embeddings: reclaimed.embeddings,
            total: reclaimed.total(),
        }
    }
}

/// Unregister a root and kill its queued scans.
///
/// # Errors
pub async fn remove(state: &AppState, path: &str) -> Result<RemoveReport, Failure> {
    let removal = fs3_store::remove_root(&state.db, path)
        .await
        .map_err(IntoFailure::into_failure)?;

    // The paths that ARE registered, gathered only when the caller's did not
    // match. Roots are stored as the daemon resolved them at `add` time — on
    // macOS `/tmp/x` is registered as `/private/tmp/x` — so "not registered"
    // is far more often a path that does not match than a root nobody added.
    // Listing them turns a dead end into a copyable answer, and it costs
    // nothing on the path that succeeded.
    let registered = if removal.was_registered() {
        Vec::new()
    } else {
        fs3_store::list_worktrees(&state.db)
            .await
            .map_err(IntoFailure::into_failure)?
            .into_iter()
            .map(|worktree| worktree.root_path)
            .collect()
    };

    // Counted AFTER the removal, so the number reflects the world the caller is
    // now in rather than the one they were in a moment ago.
    let reclaimable = fs3_store::reclaimable(&state.db)
        .await
        .map_err(IntoFailure::into_failure)?;

    Ok(RemoveReport {
        root_path: path.to_string(),
        was_registered: removal.was_registered(),
        identity: removal.identity,
        files: removal.files,
        jobs_killed: removal.jobs_killed,
        repo_removed: removal.repo_removed,
        reclaimable: reclaimable.into(),
        registered,
    })
}

/// Run a collection now.
///
/// The same engine the [`crate::gc::GcSupervisor`] runs on its cadence, called
/// directly rather than by nudging the loop. Two reasons: the reconcile runner
/// deliberately has no nudge handle, and a caller who typed `gc` wants a NUMBER
/// back, not a promise that something will happen shortly. Exactly the
/// `doctor upgrade` precedent.
///
/// # Errors
/// [`catalog::STORE_QUERY_FAILED`] when a statement fails. Batches already
/// committed stay committed.
pub async fn collect(state: &AppState) -> Result<GcCounts, Failure> {
    fs3_store::collect_garbage(&state.db)
        .await
        .map(Into::into)
        .map_err(IntoFailure::into_failure)
}

/// What a caller typically does after removing.
#[must_use]
pub fn next_after_remove(report: &RemoveReport) -> String {
    if !report.was_registered {
        return match report.registered.as_slice() {
            [] => format!(
                "{} was not registered, and neither is anything else — `flowspace3 add <path>` \
                 indexes a directory",
                report.root_path
            ),
            roots => format!(
                "{} is not registered. These are: {} — paths are stored as the daemon resolved \
                 them, so copy one exactly",
                report.root_path,
                roots.join(", ")
            ),
        };
    }

    if report.reclaimable.total == 0 {
        return "removed — nothing left to reclaim, because its content is still held by \
                another root or was never indexed"
            .to_string();
    }

    format!(
        "removed — {} row(s) are now reclaimable; garbage collection runs on its own \
         schedule, or `flowspace3 gc` does it now",
        report.reclaimable.total
    )
}

/// What a caller typically does after collecting.
#[must_use]
pub fn next_after_gc(counts: &GcCounts) -> String {
    if counts.total == 0 {
        return "nothing to reclaim — every stored row is still referenced by a registered root"
            .to_string();
    }
    format!(
        "reclaimed {} row(s): {} queued job(s), {} element(s), {} summary/summaries, {} vector(s)",
        counts.total, counts.jobs, counts.elements, counts.summaries, counts.embeddings
    )
}

/// The failure a caller gets when they name a path that is not absolute.
///
/// The daemon resolves nothing: a relative path would resolve against the
/// DAEMON's working directory, which is the trap `add` already closes at the
/// source.
#[must_use]
pub fn must_be_absolute(path: &str) -> Failure {
    Failure::new(
        &catalog::SCAN_ROOT_NOT_FOUND,
        format!("{path} is not an absolute path"),
    )
    .with_fix(
        "pass an ABSOLUTE path — a relative one would resolve against the daemon's working \
         directory, not yours",
    )
}
