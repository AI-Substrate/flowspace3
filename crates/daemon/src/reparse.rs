//! Bring every live blob to the current parser, after a parser bump.
//!
//! A bump changes [`crate::scan::PARSER_VERSION`], and a root scan re-enqueues
//! each blob with no rows under it. But nothing scans a registered root on its
//! own: `add`, `flowspace3 scan` and newly discovered worktrees do, and the
//! watcher scans only the files that change. So the @4 bump reached 2,214 of
//! 29,821 live blobs on prod (2026-10-15), and the queue read as drained
//! because almost nothing had been enqueued.
//!
//! Each pass does one bounded step:
//!
//! 1. Until it first finds nothing, delete a batch of superseded rows on blobs
//!    the current parser has already parsed. Scans prune their own blob from
//!    now on; this clears what earlier scans left. Once per process, because a
//!    pass that finds nothing reads the whole table.
//! 2. Rescan one root holding a blob that an older parser parsed and this one
//!    has not, through [`crate::roots::rescan_root`]: the same walk, diff and
//!    enqueue as `flowspace3 scan`. Each root is rescanned at most once per
//!    process, so a file that cannot be parsed cannot turn this into a loop.
//!
//! When neither has anything to do it rests for [`IDLE_TICKS`].

use std::collections::HashSet;
use std::path::Path;

use crate::reconcile::{Pass, Reconcile};
use crate::scan::PARSER_VERSION;
use crate::wiring::AppState;

/// Superseded rows deleted per pass. On prod a 1,000-row batch took 130 ms
/// while there were rows to find (2026-10-15).
const PRUNE_BATCH: i64 = 1_000;

/// Ticks to rest once every root is current: an hour at the five-second
/// cadence. A bump only ever arrives with a new process, so this re-check is
/// for a root registered while an older daemon still wrote the store.
const IDLE_TICKS: u32 = 720;

/// Rescans roots a parser bump left behind, and prunes superseded parses.
pub struct ReparseSupervisor {
    state: AppState,
    /// Roots already rescanned by this process.
    rescanned: HashSet<String>,
    /// Whether a prune batch has found nothing left.
    pruned: bool,
    /// Ticks still to skip before the next probe.
    idle: u32,
}

impl ReparseSupervisor {
    /// A supervisor over `state`'s store and roots.
    #[must_use]
    pub fn new(state: AppState) -> Self {
        Self {
            state,
            rescanned: HashSet::new(),
            pruned: false,
            idle: 0,
        }
    }
}

#[async_trait::async_trait]
impl Reconcile for ReparseSupervisor {
    fn name(&self) -> &'static str {
        "reparse"
    }

    async fn reconcile(&mut self) -> anyhow::Result<Pass> {
        if self.idle > 0 {
            self.idle -= 1;
            return Ok(Pass::QUIET);
        }

        if !self.pruned {
            let pruned =
                fs3_store::prune_superseded_batch(&self.state.db, PARSER_VERSION, PRUNE_BATCH)
                    .await?;
            if pruned > 0 {
                tracing::info!(pruned, "deleted element rows of a superseded parse");
                return Ok(Pass::changed(usize::try_from(pruned).unwrap_or(usize::MAX)));
            }
            self.pruned = true;
        }

        let stale =
            fs3_store::roots_parsed_by_an_older_parser(&self.state.db, PARSER_VERSION).await?;
        let Some(root) = stale
            .into_iter()
            .find(|root| !self.rescanned.contains(root))
        else {
            self.idle = IDLE_TICKS;
            return Ok(Pass::QUIET);
        };
        self.rescanned.insert(root.clone());
        match crate::roots::rescan_root(&self.state, Path::new(&root)).await {
            Ok(report) => tracing::info!(
                root = %report.root_path,
                files = report.files,
                enqueued = report.enqueued,
                parser_version = PARSER_VERSION,
                "rescanned a root an older parser left behind"
            ),
            // A root that is gone is the worktree supervisor's to remove; this
            // one does not try it again.
            Err(error) => tracing::warn!(
                %root,
                %error,
                "cannot rescan a root an older parser left behind"
            ),
        }
        Ok(Pass::changed(1))
    }
}
