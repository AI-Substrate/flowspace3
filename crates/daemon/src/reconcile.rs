//! The house synchronization pattern, written once.
//!
//! Ruled by Jordan, 2026-08-26 (`docs/plans/prd/daemon-worker-architecture.md`):
//! **state lives in Postgres; every consumer runs a reconcile loop against it;
//! events, where they exist at all, only wake the loop early and never carry
//! the truth.** Level-triggered correctness, edge-triggered latency.
//!
//! One pass is idempotent: read what is DESIRED from the store, look at what is
//! ACTUAL, apply the difference. A pass that runs twice changes nothing the
//! second time, which is what makes a dropped event survivable and a restart
//! uneventful.
//!
//! # Scope guard
//!
//! Reconcile loops synchronize STATE. They never dispatch WORK — the queue's
//! `SKIP LOCKED` claim loop owns that, at its own cadence ([`crate::runner`]).
//! A reconciler may ENQUEUE, because a queue row is state; it may not run the
//! job.
//!
//! One trait method, one runner, no associated types. More structure is earned
//! at the third implementor, not predicted at the first.

use std::time::Duration;

use async_trait::async_trait;

/// What one pass did, so the log can be quiet when nothing happened.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Pass {
    /// How many things this pass had to change to make actual match desired.
    ///
    /// Zero is the healthy steady state and the reason the per-pass line is
    /// `debug` rather than `info`: a loop that logged every quiet pass would
    /// bury the one pass that did something.
    pub changed: usize,
}

impl Pass {
    /// A pass that found nothing to do.
    pub const QUIET: Self = Self { changed: 0 };

    /// A pass that changed `changed` things.
    #[must_use]
    pub const fn changed(changed: usize) -> Self {
        Self { changed }
    }
}

/// One thing that keeps some actual state matching what Postgres says it
/// should be.
#[async_trait]
pub trait Reconcile: Send {
    /// The name this loop is logged under. Stable — it appears in operator
    /// output.
    fn name(&self) -> &'static str;

    /// Make actual match desired, once.
    ///
    /// # Errors
    /// Anything that stopped this pass. The runner logs it and tries again on
    /// the next tick; a reconciler must never treat a transient store outage
    /// as fatal, because the next pass is the recovery mechanism.
    async fn reconcile(&mut self) -> anyhow::Result<Pass>;
}

/// Run every reconciler forever, `every` apart, starting immediately.
///
/// The first tick of a `tokio` interval fires at once, so the boot pass falls
/// out of the cadence rather than being a special case — which matters, because
/// "watch what is already in the database when the daemon starts" and "watch
/// what was added a moment ago" then have exactly one implementation.
///
/// # Error containment
/// A failed pass is logged and the loop waits for the next tick. One
/// reconciler's bad day never stops another's, and nothing can kill the loop:
/// a daemon whose reconciler died would keep serving while silently drifting,
/// which is worse than either working or crashing.
///
/// # Shutdown
/// There is none, deliberately. This matches [`crate::runner::run_forever`],
/// the daemon's only other long-lived loop: both are `tokio::spawn`ed from the
/// composition root and both end when the process does. A shutdown handle that
/// nothing triggers is the speculative generality the doctrine's own scope
/// guard refuses. What makes that safe here is ownership — a reconciler holding
/// OS resources (the watcher does) releases them by `Drop`, so process exit is
/// already clean. If a graceful path is ever needed it belongs on
/// `http::serve`'s existing ctrl-c future, wired to both loops at once.
pub async fn run_forever(mut loops: Vec<Box<dyn Reconcile>>, every: Duration) {
    if loops.is_empty() {
        return;
    }

    let mut ticker = tokio::time::interval(every);
    // A stalled runtime must not produce a burst of catch-up passes: a late
    // pass is still just one pass, because each pass is a full comparison
    // rather than an increment.
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        ticker.tick().await;
        for reconciler in &mut loops {
            let name = reconciler.name();
            match reconciler.reconcile().await {
                Ok(Pass { changed: 0 }) => tracing::debug!(loop_name = name, "reconciled"),
                Ok(pass) => tracing::debug!(loop_name = name, changed = pass.changed, "reconciled"),
                Err(error) => tracing::error!(
                    loop_name = name,
                    %error,
                    "reconcile pass failed — retrying on the next tick"
                ),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Instant;

    use super::*;

    /// A reconciler that counts its passes and fails the ones it is told to.
    struct Counting {
        passes: Arc<AtomicUsize>,
        fail_every: usize,
    }

    #[async_trait]
    impl Reconcile for Counting {
        fn name(&self) -> &'static str {
            "counting"
        }

        async fn reconcile(&mut self) -> anyhow::Result<Pass> {
            let pass = self.passes.fetch_add(1, Ordering::SeqCst) + 1;
            if self.fail_every > 0 && pass.is_multiple_of(self.fail_every) {
                anyhow::bail!("this pass was told to fail");
            }
            Ok(Pass::changed(1))
        }
    }

    /// Wait for a condition, polling, so the timing tests state a PROPERTY
    /// rather than a schedule. `tokio`'s paused clock would be tighter, but it
    /// needs the `test-util` feature, and a dependency feature is a poor trade
    /// for two waits measured in milliseconds.
    async fn within(limit: Duration, mut done: impl FnMut() -> bool) -> bool {
        let deadline = Instant::now() + limit;
        while Instant::now() < deadline {
            if done() {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        done()
    }

    #[tokio::test]
    async fn the_first_pass_runs_at_boot_without_waiting_for_a_tick() {
        let passes = Arc::new(AtomicUsize::new(0));
        let reconciler = Counting {
            passes: Arc::clone(&passes),
            fail_every: 0,
        };

        // An hour of cadence. Any pass observed inside half a second can only
        // be the immediate one, which is what makes "watch what was already
        // registered at boot" free rather than a special case.
        let handle = tokio::spawn(run_forever(
            vec![Box::new(reconciler)],
            Duration::from_secs(3_600),
        ));
        let counted = &passes;
        assert!(
            within(Duration::from_millis(500), || counted
                .load(Ordering::SeqCst)
                >= 1)
            .await,
            "boot must not wait a full cadence to look at the world"
        );
        assert_eq!(
            passes.load(Ordering::SeqCst),
            1,
            "and exactly one pass — the hour has not elapsed"
        );
        handle.abort();
    }

    #[tokio::test]
    async fn a_failing_pass_never_kills_the_loop() {
        let passes = Arc::new(AtomicUsize::new(0));
        let reconciler = Counting {
            passes: Arc::clone(&passes),
            // Every pass fails. If failure could stop the loop, it would stop
            // on the first one.
            fail_every: 1,
        };

        let handle = tokio::spawn(run_forever(
            vec![Box::new(reconciler)],
            Duration::from_millis(5),
        ));
        let counted = &passes;
        assert!(
            within(Duration::from_secs(5), || counted.load(Ordering::SeqCst)
                >= 4)
            .await,
            "the loop must keep ticking through consecutive failures"
        );
        handle.abort();
    }

    #[tokio::test]
    async fn an_empty_roster_returns_instead_of_spinning() {
        // Not a no-op forever: a runner with nothing to run should let its task
        // finish, so an empty roster costs no timer.
        tokio::time::timeout(Duration::from_secs(1), run_forever(vec![], Duration::ZERO))
            .await
            .expect("an empty roster returns immediately");
    }
}
