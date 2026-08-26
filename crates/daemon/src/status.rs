//! `GET /status`: what is registered, and what is left to do.
//!
//! The two questions an operator actually has after `add`, answered together
//! because they are one question: "is it working, and is it done?" Roots without
//! queue depth reads as done when nothing has started; queue depth without roots
//! reads as broken when nothing was ever added.

use fs3_core::envelope::Failure;
use serde::{Deserialize, Serialize};

use crate::answer::IntoFailure;
use crate::wiring::AppState;

/// What `GET /status` answers with.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusReport {
    /// Every registered worktree, with its file count.
    pub roots: Vec<Root>,
    /// The queue, grouped by kind and state.
    pub queue: Vec<QueueRow>,
    /// The most recent failure, when there is one — so a status line can say
    /// what went wrong rather than only that something did.
    pub last_error: Option<LastError>,
    /// Migrations the DATABASE has that this binary does not.
    ///
    /// Empty in the normal case. Non-empty means a newer daemon has migrated
    /// this database, which is worth saying out loud: it explains a column
    /// nobody here expects, and it is the first thing to check when two daemons
    /// disagree.
    pub schema_ahead: Vec<i64>,
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

/// Read the current state.
///
/// # Errors
/// Store failures, mapped to their own catalog codes.
pub async fn report(state: &AppState) -> Result<StatusReport, Failure> {
    let roots = fs3_store::list_worktrees(&state.db)
        .await
        .map_err(IntoFailure::into_failure)?
        .into_iter()
        .map(|worktree| Root {
            identity: worktree.identity,
            root_path: worktree.root_path,
            files: worktree.file_count,
        })
        .collect();

    let queue = fs3_store::queue_depth(&state.db)
        .await
        .map_err(IntoFailure::into_failure)?
        .into_iter()
        .map(|row| QueueRow {
            kind: row.kind,
            state: row.state,
            count: row.depth,
            with_error: row.with_error,
        })
        .collect();

    let last_error = fs3_store::last_failure(&state.db)
        .await
        .map_err(IntoFailure::into_failure)?
        .map(|(job, error)| LastError { job, error });

    Ok(StatusReport {
        roots,
        queue,
        last_error,
        schema_ahead: crate::schema::ahead_of_us(&state.db).await,
    })
}
