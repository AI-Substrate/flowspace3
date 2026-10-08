//! Classify the conversation turns stored before turn classes existed.
//!
//! New turns are classified as they are appended. Turns already in the store
//! when migration 0025 ran carry `turn_class = NULL`: never hidden, so search
//! is correct but still shows their echoes until this catches up. It works
//! through them a few conversations per tick on the shared reconcile cadence,
//! resumes after a restart for free (the queue IS the NULL rows), and costs
//! one empty index probe per tick once it is done.

use fs3_store::PgPool;

use crate::reconcile::{Pass, Reconcile};

/// Conversations classified per tick. A tick is five seconds; the largest
/// measured conversation is ~15k turns, read in 1k-turn windows.
const CONVERSATIONS_PER_TICK: u32 = 10;

/// Drives [`fs3_store::backfill_turn_classes`] until nothing is left.
pub struct TurnClassBackfill {
    pool: PgPool,
}

impl TurnClassBackfill {
    /// A backfill over `pool`.
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl Reconcile for TurnClassBackfill {
    fn name(&self) -> &'static str {
        "turn-class-backfill"
    }

    async fn reconcile(&mut self) -> anyhow::Result<Pass> {
        let classified =
            fs3_store::backfill_turn_classes(&self.pool, CONVERSATIONS_PER_TICK).await?;
        if classified == 0 {
            return Ok(Pass::QUIET);
        }
        tracing::info!(classified, "classified stored conversation turns");
        Ok(Pass::changed(usize::try_from(classified).expect(
            "a pass classifies far fewer rows than usize holds",
        )))
    }
}
