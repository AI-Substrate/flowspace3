//! Restate the conversation turns stored before the 2026-10-13 intake fixes.
//!
//! Migration 0030 queued every stored conversation. This works through them a
//! window of turns at a time on the shared reconcile cadence, with the rules in
//! [`fs3_providers::conversation_sources::restate`]. Most corrections are a
//! turn's role or source and leave its text, hash and vectors alone; a
//! write-family turn that held a whole file body gets new, shorter text, and
//! that text is queued for a vector and, above the floor, a summary.
//!
//! That enrichment shares the queue with live work, so a tick is skipped while
//! more than [`PENDING_CEILING`] embed and summarize jobs wait: the backfill
//! trickles in behind what is happening now. It resumes after a restart from
//! each conversation's cursor, and costs one empty index probe per tick once
//! it is done.

use fs3_providers::conversation_sources::restate::restate_stored_turn;

use crate::conversations::UNANCHORED;
use crate::enrich::{EMBED, SUMMARIZE};
use crate::reconcile::{Pass, Reconcile};
use crate::wiring::AppState;

/// Pending embed and summarize jobs above which a tick restates nothing.
pub const PENDING_CEILING: i64 = 500;

/// Windows of up to 1,000 turns restated per tick. A tick is five seconds.
const WINDOWS_PER_TICK: u32 = 10;

/// Drives [`fs3_store::restate_next_window`] until no conversation is queued.
pub struct IntakeRestate {
    state: AppState,
    ceiling: i64,
}

impl IntakeRestate {
    /// A restatement over `state`'s store, held back by [`PENDING_CEILING`].
    #[must_use]
    pub fn new(state: AppState) -> Self {
        Self::with_ceiling(state, PENDING_CEILING)
    }

    /// A restatement held back once more than `ceiling` enrichment jobs wait.
    #[must_use]
    pub fn with_ceiling(state: AppState, ceiling: i64) -> Self {
        Self { state, ceiling }
    }
}

/// Embed and summarize jobs waiting to run.
async fn pending_enrichment(state: &AppState) -> anyhow::Result<i64> {
    Ok(fs3_store::queue_depth(&state.db)
        .await?
        .iter()
        .filter(|row| row.state == "pending" && (row.kind == EMBED || row.kind == SUMMARIZE))
        .map(|row| row.depth)
        .sum())
}

#[async_trait::async_trait]
impl Reconcile for IntakeRestate {
    fn name(&self) -> &'static str {
        "intake-restate"
    }

    async fn reconcile(&mut self) -> anyhow::Result<Pass> {
        let floor = self.state.config.indexing.turn_summary_min_bytes;
        let (mut restated, mut reenriched, mut finished) = (0_u64, 0_usize, 0_u64);
        for _ in 0..WINDOWS_PER_TICK {
            let pending = pending_enrichment(&self.state).await?;
            if pending > self.ceiling {
                tracing::debug!(
                    pending,
                    ceiling = self.ceiling,
                    "intake restatement waiting for the enrichment queue"
                );
                break;
            }
            let Some(window) = fs3_store::restate_next_window(
                &self.state.db,
                restate_stored_turn,
                |element: &fs3_core::Element| {
                    fs3_core::conversation::earns_summary(&element.raw_text, floor)
                },
            )
            .await?
            else {
                break;
            };
            let identity = window.repo_identity.as_deref().map_or_else(
                || UNANCHORED.to_string(),
                fs3_core::RepoIdentity::normalise_key,
            );
            crate::enrich::enqueue_for_turns(&self.state, &identity, &window.replaced, floor)
                .await
                .map_err(|failure| anyhow::anyhow!("{failure}"))?;
            restated += window.restated;
            reenriched += window.replaced.len();
            finished += u64::from(window.finished);
        }
        if restated == 0 && finished == 0 {
            return Ok(Pass::QUIET);
        }
        let remaining = fs3_store::conversations_to_restate(&self.state.db).await?;
        tracing::info!(
            restated,
            reenriched,
            conversations_finished = finished,
            conversations_remaining = remaining,
            "restated stored conversation turns"
        );
        Ok(Pass::changed(
            usize::try_from(restated).unwrap_or(usize::MAX),
        ))
    }
}
