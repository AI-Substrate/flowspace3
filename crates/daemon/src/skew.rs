//! The schema producer: telling a RUNNING daemon that its database moved on
//! without it (PRD reqs 59, 61).
//!
//! # The case this exists for, and the case it deliberately does not
//!
//! Boot already refuses a database that is ahead of this binary, loudly and
//! with the fix ([`fs3_core::skew`]). That is not this. A boot failure cannot
//! be a queue producer at all: the process exits, so nothing is left to RETRACT
//! the message when the situation resolves — and the binary that hits it is by
//! definition old enough that it may not even know the queue exists.
//!
//! This is the sibling nobody was watching. A daemon boots cleanly against a
//! schema it understands, and then somebody runs `flowspace3 doctor` from a
//! newer binary, or a colleague starts a newer daemon against the same store.
//! The migrations land, and this process keeps serving — reading and writing a
//! schema it does not fully understand — with nothing anywhere saying so. The
//! fact was computable the whole time ([`crate::schema::ahead_of_us`]) and
//! surfaced only as a field in `flowspace3 status` that you had to know to go
//! and look at.
//!
//! # Why a reconcile loop is the right shape
//!
//! The condition ARRIVES while the process runs and can DISAPPEAR the same way
//! — the newer daemon is stopped, or this one is restarted into a newer binary.
//! A pass declares what is true now and the queue retracts the rest, so the
//! message appears and vanishes on its own with no clear-condition machinery.
//! That is the same contract the update supervisor runs under, arrived at from
//! the opposite direction, which is what makes it a real test of the seam
//! rather than a second copy of the first producer.

use anyhow::Result;

use crate::reconcile::{Pass, Reconcile};

/// Watches for the database getting ahead of this binary, and says so.
pub struct SchemaSupervisor {
    pool: fs3_store::PgPool,
    running: String,
}

impl SchemaSupervisor {
    /// Watch `pool` on behalf of a binary reporting `running`.
    #[must_use]
    pub fn new(pool: fs3_store::PgPool, running: &str) -> Self {
        Self {
            pool,
            running: running.to_string(),
        }
    }
}

#[async_trait::async_trait]
impl Reconcile for SchemaSupervisor {
    fn name(&self) -> &'static str {
        "schema"
    }

    async fn reconcile(&mut self) -> Result<Pass> {
        let status = fs3_store::schema_current(&self.pool).await?;
        let skew = status.skew(&self.running);
        let desired = skew.desired_messages();

        // `changed` counts what a pass had to DO. Declaring the same thing
        // again is not a change, and the steady state here is "nothing is
        // wrong" — so a healthy daemon logs a quiet pass forever, which is
        // exactly what the runner's debug-level line was designed for.
        let changed = usize::from(skew.is_skewed());

        if skew.is_skewed() {
            tracing::error!(
                extra = %skew.extra_summary(),
                running = %self.running,
                "the database has migrations this binary does not — it moved on without us"
            );
        }

        // `None` scope: schema skew is a property of the STORE, so it is news
        // for every installation pointed at it, not for one install path.
        fs3_store::sync_messages(&self.pool, fs3_core::SCHEMA_SOURCE, None, &desired).await?;
        Ok(Pass::changed(changed))
    }
}
