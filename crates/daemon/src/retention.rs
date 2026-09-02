//! Bounded lifecycle for completed jobs.
//!
//! The shared reconcile runner supplies the clock. This supervisor runs on its
//! first tick at boot, then hourly, and drains expired rows through short,
//! bounded delete statements. Snap-in recipe for the composition root:
//!
//! ```ignore
//! reconcilers.push(Box::new(crate::retention::RetentionSupervisor::new(
//!     state.db.clone(),
//!     state.config.indexing.job_retention_days,
//! )));
//! ```

use std::{num::NonZeroU32, time::Duration};

use fs3_store::{JobRetentionReceipt, PgPool};

use crate::reconcile::{Pass, Reconcile};

const RETENTION_EVERY_TICKS: u32 = 720;
const PURGE_BATCH: NonZeroU32 = NonZeroU32::new(10_000).expect("the purge batch is non-zero");
const SECONDS_PER_DAY: u64 = 86_400;

/// Purges expired completed jobs without retaining a process-local shadow count.
pub struct RetentionSupervisor {
    pool: PgPool,
    window_days: u32,
    ticks: u32,
}

impl RetentionSupervisor {
    /// Build a supervisor whose first shared runner tick performs the boot sweep.
    #[must_use]
    pub fn new(pool: PgPool, window_days: u32) -> Self {
        Self {
            pool,
            window_days,
            ticks: RETENTION_EVERY_TICKS - 1,
        }
    }

    fn due(&mut self) -> bool {
        tick_due(&mut self.ticks)
    }
}

fn tick_due(ticks: &mut u32) -> bool {
    *ticks = ticks.saturating_add(1);
    if *ticks < RETENTION_EVERY_TICKS {
        return false;
    }
    *ticks = 0;
    true
}

#[async_trait::async_trait]
impl Reconcile for RetentionSupervisor {
    fn name(&self) -> &'static str {
        "jobs-retention"
    }

    async fn reconcile(&mut self) -> anyhow::Result<Pass> {
        if !self.due() {
            return Ok(Pass::QUIET);
        }
        let receipt = sweep_once(&self.pool, self.window_days).await?;
        tracing::info!(
            window_days = self.window_days,
            purged = receipt.purged_last_run,
            last_purge_at = receipt.last_purge_at.as_deref().unwrap_or("unknown"),
            "purged expired done jobs"
        );
        Ok(Pass::changed(
            usize::try_from(receipt.purged_last_run)
                .expect("Postgres cannot delete more rows than this process can address"),
        ))
    }
}

/// Complete one sweep using as many bounded statements as needed.
///
/// The receipt is written only after the last batch. Re-running after a
/// completed sweep is idempotent and records a zero-row result.
///
/// # Errors
/// Store failures from either a delete batch or the final receipt write.
pub async fn sweep_once(pool: &PgPool, window_days: u32) -> anyhow::Result<JobRetentionReceipt> {
    let older_than = Duration::from_secs(u64::from(window_days) * SECONDS_PER_DAY);
    let mut total = 0_u64;
    loop {
        let purged = fs3_store::purge_done_jobs(pool, older_than, PURGE_BATCH).await?;
        total = total
            .checked_add(purged)
            .expect("a sweep cannot exceed the number of rows Postgres can hold");
        if purged < u64::from(PURGE_BATCH.get()) {
            break;
        }
    }
    let at = fs3_store::record_job_retention(pool, total).await?;
    Ok(JobRetentionReceipt {
        last_purge_at: Some(at),
        purged_last_run: total,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_pass_is_due_then_the_hourly_cadence_holds() {
        let mut ticks = RETENTION_EVERY_TICKS - 1;
        assert!(tick_due(&mut ticks));
        for _ in 1..RETENTION_EVERY_TICKS {
            assert!(!tick_due(&mut ticks));
        }
        assert!(tick_due(&mut ticks));
    }
}
