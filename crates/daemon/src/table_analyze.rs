//! Refresh a big table's planner statistics once bulk work has changed it.
//!
//! A backfill, a restatement or a large import can change hundreds of
//! thousands of rows, and a plan chosen on the statistics from before it can
//! be catastrophically wrong: on 2026-10-14 a one-row probe on stale
//! statistics read the whole `elements` table every five seconds. Migration
//! 0031 makes autovacuum analyze sooner; this does it the moment the queue is
//! idle after the work, rather than whenever an autovacuum worker gets there.

use fs3_store::PgPool;

use crate::reconcile::{Pass, Reconcile};

/// A table is stale once this share of its rows changed since its last ANALYZE…
const STALE_SHARE: f64 = 0.01;
/// …and at least this many, so a small table is left to autovacuum.
const STALE_ROWS: i64 = 10_000;

/// ANALYZEs each watched table that bulk work has made stale, while the job
/// queue is idle.
pub struct TableAnalyze {
    pool: PgPool,
}

impl TableAnalyze {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

/// Whether `health` has changed enough since its last ANALYZE to need one.
#[must_use]
pub fn is_stale(health: &fs3_store::TableHealth) -> bool {
    health.changed_since_analyze >= STALE_ROWS && health.stale_ratio() >= STALE_SHARE
}

#[async_trait::async_trait]
impl Reconcile for TableAnalyze {
    fn name(&self) -> &'static str {
        "table-analyze"
    }

    async fn reconcile(&mut self) -> anyhow::Result<Pass> {
        // Bulk work is still landing while jobs wait; statistics taken now
        // would be stale again within the minute.
        let busy = fs3_store::queue_depth(&self.pool)
            .await?
            .iter()
            .any(|row| (row.state == "pending" || row.state == "running") && row.depth > 0);
        if busy {
            return Ok(Pass::QUIET);
        }
        let mut analyzed = 0;
        for health in fs3_store::table_health(&self.pool).await? {
            if !is_stale(&health) {
                continue;
            }
            let started = std::time::Instant::now();
            fs3_store::analyze_table(&self.pool, &health.table).await?;
            tracing::info!(
                table = %health.table,
                changed = health.changed_since_analyze,
                live = health.live_rows,
                ms = started.elapsed().as_millis(),
                "analyzed a table bulk work had changed"
            );
            analyzed += 1;
        }
        Ok(Pass::changed(analyzed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn health(live_rows: i64, changed_since_analyze: i64) -> fs3_store::TableHealth {
        fs3_store::TableHealth {
            table: "elements".to_string(),
            live_rows,
            dead_rows: 0,
            changed_since_analyze,
            analyzed_secs_ago: Some(60),
            vacuumed_secs_ago: Some(60),
        }
    }

    #[test]
    fn stale_needs_both_a_share_and_a_floor_of_changed_rows() {
        assert!(is_stale(&health(1_250_000, 45_000)), "3.6% of a big table");
        assert!(!is_stale(&health(1_250_000, 9_000)), "under 1%");
        assert!(
            !is_stale(&health(20_000, 9_000)),
            "a small table is autovacuum's"
        );
        assert!(is_stale(&health(20_000, 10_000)));
    }
}
