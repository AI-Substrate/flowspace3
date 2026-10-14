//! Classify the conversation turns stored before turn classes existed.
//!
//! New turns are classified as they are appended. Turns already in the store
//! when migration 0025 ran carry `turn_class = NULL`: never hidden, so search
//! is correct but still shows their echoes until this catches up. It works
//! through them a few conversations per tick on the shared reconcile cadence,
//! and resumes after a restart for free (the queue IS the NULL rows).
//!
//! Once a pass finds nothing left it checks again only every [`IDLE_TICKS`]
//! ticks. It used to probe every tick forever, and on prod each probe read the
//! whole `elements` table (2026-10-14), evicting the vector index searches
//! need. New turns are classified as they are appended, so a finished backfill
//! only re-checks for a row some other path left unclassified.

use fs3_store::PgPool;

use crate::reconcile::{Pass, Reconcile};

/// Conversations classified per tick. A tick is five seconds; the largest
/// measured conversation is ~15k turns, read in 1k-turn windows.
const CONVERSATIONS_PER_TICK: u32 = 10;

/// Ticks to rest after a pass finds nothing left to classify: an hour at the
/// five-second cadence.
const IDLE_TICKS: u32 = 720;

/// Drives [`fs3_store::backfill_turn_classes`] until nothing is left.
pub struct TurnClassBackfill {
    pool: PgPool,
    /// Ticks still to skip before the next probe.
    idle: u32,
}

impl TurnClassBackfill {
    /// A backfill over `pool`.
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool, idle: 0 }
    }
}

#[async_trait::async_trait]
impl Reconcile for TurnClassBackfill {
    fn name(&self) -> &'static str {
        "turn-class-backfill"
    }

    async fn reconcile(&mut self) -> anyhow::Result<Pass> {
        if self.idle > 0 {
            self.idle -= 1;
            return Ok(Pass::QUIET);
        }
        let classified =
            fs3_store::backfill_turn_classes(&self.pool, CONVERSATIONS_PER_TICK).await?;
        if classified == 0 {
            self.idle = IDLE_TICKS;
            return Ok(Pass::QUIET);
        }
        tracing::info!(classified, "classified stored conversation turns");
        Ok(Pass::changed(usize::try_from(classified).expect(
            "a pass classifies far fewer rows than usize holds",
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A pool that cannot connect: any probe through it fails, so a quiet
    /// pass proves no probe was made.
    fn unreachable() -> PgPool {
        fs3_store::connect_lazy("postgres://fs3:fs3@127.0.0.1:1/none").expect("a lazy pool")
    }

    #[tokio::test]
    async fn a_finished_backfill_rests_before_probing_again() {
        let mut backfill = TurnClassBackfill::new(unreachable());
        backfill.idle = 2;
        for _ in 0..2 {
            let pass = backfill
                .reconcile()
                .await
                .expect("a resting pass makes no probe");
            assert_eq!(pass.changed, 0);
        }
        assert!(
            backfill.reconcile().await.is_err(),
            "after resting it probes the store again"
        );
    }
}
