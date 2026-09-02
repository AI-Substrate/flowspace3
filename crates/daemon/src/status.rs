//! `GET /status`: what is registered, and what is left to do.
//!
//! The two questions an operator actually has after `add`, answered together
//! because they are one question: "is it working, and is it done?" Roots without
//! queue depth reads as done when nothing has started; queue depth without roots
//! reads as broken when nothing was ever added.

use fs3_core::envelope::Failure;

use crate::answer::IntoFailure;
use crate::wiring::AppState;
use fs3_core::views::status::{
    ElementTreeInconsistency, LastError, QueueRow, RetentionStatus, Root, StatusReport,
};

pub async fn report(state: &AppState, history: bool) -> Result<StatusReport, Failure> {
    let roots = fs3_store::list_worktrees(&state.db)
        .await
        .map_err(IntoFailure::into_failure)?
        .into_iter()
        .map(|worktree| Root {
            identity: worktree.identity,
            root_path: worktree.root_path,
            include_hidden: worktree.include_hidden,
            files: worktree.file_count,
        })
        .collect();

    let depth = if history {
        fs3_store::queue_depth_history(&state.db).await
    } else {
        fs3_store::queue_depth(&state.db).await
    };
    let queue = depth
        .map_err(IntoFailure::into_failure)?
        .into_iter()
        .map(|row| QueueRow {
            kind: row.kind,
            state: row.state,
            count: row.depth,
            with_error: row.with_error,
        })
        .collect();

    let retained = fs3_store::job_retention_receipt(&state.db)
        .await
        .map_err(IntoFailure::into_failure)?;
    let retention = RetentionStatus {
        window_days: state.config.indexing.job_retention_days,
        last_purge_at: retained.last_purge_at,
        purged_last_run: retained.purged_last_run,
    };

    let last_error = fs3_store::last_failure(&state.db)
        .await
        .map_err(IntoFailure::into_failure)?
        .map(|(job, error)| LastError { job, error });

    let inconsistencies = fs3_store::element_tree_inconsistencies(&state.db)
        .await
        .map_err(IntoFailure::into_failure)?
        .into_iter()
        .map(|issue| ElementTreeInconsistency {
            blob_sha: issue.blob_sha,
            parser_version: issue.parser_version,
            paths: issue.paths,
            next_action: "restart the current flowspace3 daemon to apply the duplicate-root repair migration; if this remains, run `flowspace3 doctor`".to_string(),
        })
        .collect();

    Ok(StatusReport {
        roots,
        queue,
        retention: Some(retention),
        last_error,
        inconsistencies,
        schema_ahead: crate::schema::ahead_of_us(&state.db).await,
    })
}
