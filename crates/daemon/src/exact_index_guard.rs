//! Keep the binary copy's ready mark honest (migration 0033).
//!
//! Search ranks an exact scope on `embeddings_1024_bq` only while that copy
//! is marked complete. The trigger keeps it complete for every write, but not
//! for what goes around it: a restore of `embeddings_1024` alone, a manual
//! delete, a load with triggers disabled. Any of those would leave sources
//! that compact ranking can never return. This re-counts the copy against
//! the vectors per embedding model at boot and every ten minutes, and clears
//! the mark the moment a vector lacks its copy, which sends search back to
//! the full vectors until `flowspace3 doctor exact-index` refills it.

use anyhow::Result;

use crate::reconcile::{Pass, Reconcile};

/// Reconcile ticks between checks: ten minutes at the five-second cadence.
/// A check is one lookup while the copy is unmarked, and a count of both
/// tables (about a second at 1.7M vectors) while it is marked.
const TICKS_BETWEEN_CHECKS: u32 = 120;

/// Clears the binary copy's ready mark when the copy has fallen behind.
pub struct ExactIndexGuard {
    pool: fs3_store::PgPool,
    /// Starts satisfied, so the first check runs at boot: a restore is
    /// followed by a start.
    ticks: u32,
}

impl ExactIndexGuard {
    #[must_use]
    pub fn new(pool: fs3_store::PgPool) -> Self {
        Self {
            pool,
            ticks: TICKS_BETWEEN_CHECKS,
        }
    }
}

#[async_trait::async_trait]
impl Reconcile for ExactIndexGuard {
    fn name(&self) -> &'static str {
        "exact-index-guard"
    }

    async fn reconcile(&mut self) -> Result<Pass> {
        self.ticks += 1;
        if self.ticks < TICKS_BETWEEN_CHECKS {
            return Ok(Pass::QUIET);
        }
        self.ticks = 0;
        let Some(coverage) = fs3_store::guard_exact_index(&self.pool).await? else {
            return Ok(Pass::QUIET);
        };
        let missing: i64 = coverage.iter().map(|model| model.missing).sum();
        tracing::warn!(
            missing,
            "vectors lack their binary copy although it was marked complete; cleared the mark, \
             so exact ranking reads full vectors until `flowspace3 doctor exact-index` refills it"
        );
        Ok(Pass::changed(1))
    }
}
