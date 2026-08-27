//! The garbage collector's reconcile loop (PRD req 57).
//!
//! Same engine, two entry points — the shape this daemon already uses for
//! auto-update and `doctor upgrade`. The cadence keeps the database tidy on its
//! own; `flowspace3 gc` is for when somebody wants it now and wants a number
//! back.
//!
//! # Why GC is a reconciler rather than removal's cleanup step
//!
//! [`fs3_store::collect_garbage`] re-derives what is unreferenced from
//! Postgres on every pass, so it is a statement about the whole database rather
//! than about the removal that happened to precede it. That means it reaps
//! residue nobody removed on purpose: a crash mid-scan, a branch switch that
//! left a blob nothing maps, an old removal from before this code existed.
//! Bolting it onto `remove` would have covered exactly one of those.
//!
//! Jordan blessed the latency explicitly (2026-08-27): "its not end of world if
//! some detritus before gc runs". So this runs SLOWLY and cheaply, and the
//! honest thing to tell a user after `remove` is what is reclaimABLE, not what
//! has been reclaimed.

use anyhow::Result;

use crate::reconcile::{Pass, Reconcile};

/// How many reconcile ticks pass between collections.
///
/// The runner has one cadence for every loop (seconds, because the watcher
/// needs it), and sweeping the whole content layer at that rate would be
/// absurd. Counting ticks keeps the interval here rather than growing the
/// trait — the same reasoning as the update supervisor's clock, arrived at for
/// the same reason.
///
/// Deliberately NOT persisted, unlike the update check. There is no quota to
/// spend and no external service to be polite to: the cost of a redundant pass
/// after a restart is one cheap query per level that finds nothing.
const TICKS_BETWEEN_PASSES: u32 = 720;

/// Reclaims what nothing references, on a slow cadence.
pub struct GcSupervisor {
    pool: fs3_store::PgPool,
    /// Ticks since the last pass. Starts satisfied so the first pass runs at
    /// boot: a daemon starting up is exactly when residue from the last one's
    /// unclean exit is sitting there.
    ticks: u32,
}

impl GcSupervisor {
    /// Watch `pool`, collecting every [`TICKS_BETWEEN_PASSES`] ticks.
    #[must_use]
    pub fn new(pool: fs3_store::PgPool) -> Self {
        Self {
            pool,
            ticks: TICKS_BETWEEN_PASSES,
        }
    }
}

#[async_trait::async_trait]
impl Reconcile for GcSupervisor {
    fn name(&self) -> &'static str {
        "gc"
    }

    async fn reconcile(&mut self) -> Result<Pass> {
        self.ticks += 1;
        if self.ticks < TICKS_BETWEEN_PASSES {
            return Ok(Pass::QUIET);
        }
        self.ticks = 0;

        let reclaimed = fs3_store::collect_garbage(&self.pool).await?;
        if reclaimed.is_empty() {
            // The steady state, and it must stay quiet: a log line every pass
            // saying "nothing to do" is how people learn to stop reading logs.
            return Ok(Pass::QUIET);
        }

        tracing::info!(
            jobs = reclaimed.jobs,
            elements = reclaimed.elements,
            summaries = reclaimed.summaries,
            embeddings = reclaimed.embeddings,
            "reclaimed rows nothing references any more"
        );
        Ok(Pass::changed(
            usize::try_from(reclaimed.total()).unwrap_or(usize::MAX),
        ))
    }
}
