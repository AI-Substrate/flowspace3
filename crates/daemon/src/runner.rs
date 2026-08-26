//! The worker loop: claim a job, run it, settle it.
//!
//! One generic runner over typed handlers (the locked direction in
//! `docs/plans/prd/daemon-worker-architecture.md`). `scan_file` is the first
//! kind; `summarize` and `embed` are siblings rather than subclasses, which is
//! what lets one file's scan fan out into many small enrichment jobs that run
//! concurrently with each other and with the next file's scan.
//!
//! # The retry policy lives HERE
//!
//! The store deliberately invents no schedule — `fail_job` is terminal and
//! `retry_job` is a verb. This module is the layer that decides: three attempts,
//! backing off, and only for failures the catalog marks retryable. That last
//! qualifier is the one that matters. Re-running a job whose cause is a missing
//! API key costs three times as much and fails three times; the `retryable` bit
//! workshop 004 put on every error is what tells the two apart, which is exactly
//! the "feeds the queue retry policy" the workshop claimed for it (D5).
//!
//! # Why claiming is a poll and not a listen
//!
//! `LISTEN`/`NOTIFY` would remove the idle latency, and it would also remove the
//! property that makes this correct: `claim_job`'s `FOR UPDATE SKIP LOCKED` is
//! what lets N workers take N different jobs without coordinating. A poll that
//! finds nothing is one indexed query; the daemon does it a few times a second
//! at most. Notification is an optimisation to measure, not a design to start
//! from.

use std::sync::Arc;
use std::time::Duration;

use fs3_core::catalog;
use fs3_core::envelope::Failure;
use fs3_store::{Job, PgPool};
use tokio::task::JoinSet;

use crate::answer::IntoFailure;
use crate::enrich::{self, EMBED, SUMMARIZE};
use crate::roots::SCAN_FILE;
use crate::wiring::AppState;

/// Every job kind the runner claims, in the order it prefers them.
///
/// Scans first: a scan produces enrichment work, so draining scans early keeps
/// the LLM and embedding calls — the slow, parallel part — fed.
pub const KINDS: &[&str] = &[SCAN_FILE, SUMMARIZE, EMBED];

/// How many times a retryable job is attempted before it is failed for good.
///
/// Three, settling plan 003's queued decision. Two is not enough to ride out a
/// provider's rate limit; ten turns a broken deployment into ten times the spend
/// and ten times the log noise before anyone is told.
pub const MAX_ATTEMPTS: i32 = 3;

/// Backoff before attempt `attempts + 1`.
///
/// Exponential from a base of one second: 2s, 4s. Short enough that a transient
/// blip clears within one status check, long enough that a rate-limited
/// provider is not hammered by the retry itself.
#[must_use]
pub fn backoff(attempts: i32) -> Duration {
    Duration::from_secs(1 << attempts.clamp(1, 6))
}

/// How long to wait after finding an empty queue before asking again.
const IDLE_POLL: Duration = Duration::from_millis(250);

/// What one drain pass did — the shape the e2e test asserts against.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Drained {
    /// Jobs that completed.
    pub completed: usize,
    /// Jobs put back for another attempt.
    pub retried: usize,
    /// Jobs that failed terminally.
    pub failed: usize,
}

impl Drained {
    /// Total jobs settled this pass.
    #[must_use]
    pub fn total(&self) -> usize {
        self.completed + self.retried + self.failed
    }

    fn absorb(&mut self, other: Drained) {
        self.completed += other.completed;
        self.retried += other.retried;
        self.failed += other.failed;
    }
}

/// Run jobs until the queue is empty, `workers` at a time.
///
/// "Empty" means nothing is *ready*: a job backing off is not ready, so a drain
/// can finish while retries are still pending. That is deliberate — the
/// alternative is a drain that blocks for the length of the longest backoff, and
/// callers who want the retry can drain again.
///
/// Returns what it settled. Errors are per-job and never abort the pass: one
/// unreadable file must not stop a repository from indexing.
pub async fn drain(state: &AppState, workers: usize) -> Drained {
    let mut total = Drained::default();
    let mut tasks = JoinSet::new();
    let workers = workers.max(1);

    loop {
        while tasks.len() < workers {
            match fs3_store::claim_job(&state.db, KINDS).await {
                Ok(Some(job)) => {
                    let state = state.clone();
                    tasks.spawn(async move { settle(&state, job).await });
                }
                Ok(None) => break,
                Err(error) => {
                    // The store is gone. Nothing else in this pass can work
                    // either, so stop rather than spin.
                    tracing::error!(%error, "cannot claim jobs");
                    break;
                }
            }
        }

        if tasks.is_empty() {
            return total;
        }
        if let Some(result) = tasks.join_next().await {
            match result {
                Ok(outcome) => total.absorb(outcome),
                // A panicking handler is a defect, and losing the job's row
                // would hide it. The row stays `running` and the reconciler
                // sweep is what recovers the work.
                Err(error) => tracing::error!(%error, "a job handler panicked"),
            }
        }
    }
}

/// Run the loop forever, for the daemon's background worker.
///
/// Sleeps only when it finds nothing, so a busy queue is never delayed by a
/// timer.
pub async fn run_forever(state: AppState, workers: usize) {
    loop {
        let drained = drain(&state, workers).await;
        if drained.total() == 0 {
            tokio::time::sleep(IDLE_POLL).await;
        }
    }
}

/// Run one job and settle its row.
async fn settle(state: &AppState, job: Job) -> Drained {
    let id = job.id;
    let attempts = job.attempts;
    let kind = job.kind.clone();
    let key = job.dedupe_key.clone();

    match dispatch(state, job).await {
        Ok(()) => {
            if let Err(error) = fs3_store::complete_job(&state.db, id).await {
                tracing::error!(%error, id, "cannot complete job");
            }
            Drained {
                completed: 1,
                ..Drained::default()
            }
        }
        Err(failure) => {
            let again = failure.retryable && attempts < MAX_ATTEMPTS;
            let message = format!("{} {}", failure.code, failure.message);
            tracing::warn!(id, %kind, %key, attempts, retrying = again, "{message}");

            let outcome = if again {
                fs3_store::retry_job(&state.db, id, backoff(attempts), &message).await
            } else {
                fs3_store::fail_job(&state.db, id, &message).await
            };
            if let Err(error) = outcome {
                tracing::error!(%error, id, "cannot settle a failed job");
            }

            if again {
                Drained {
                    retried: 1,
                    ..Drained::default()
                }
            } else {
                Drained {
                    failed: 1,
                    ..Drained::default()
                }
            }
        }
    }
}

/// Route a job to its handler.
///
/// An unknown kind fails terminally rather than retrying: a job nobody can run
/// will not become runnable by waiting, and a retry loop over it is spend with
/// no upside.
async fn dispatch(state: &AppState, job: Job) -> Result<(), Failure> {
    match job.kind.as_str() {
        SCAN_FILE => crate::scan::run(state, job.payload).await,
        SUMMARIZE => enrich::summarize(state, job.payload).await,
        EMBED => enrich::embed(state, job.payload).await,
        other => Err(Failure::new(
            &catalog::QUEUE_JOB_FAILED,
            format!("no handler for job kind {other:?}"),
        )
        .retryable(false)),
    }
}

/// Deserialise a job payload, turning a malformed one into a terminal failure.
///
/// A payload that does not parse cannot be fixed by running it again, so this
/// clears `retryable` explicitly rather than letting the catalog's default
/// decide.
pub fn payload<T: serde::de::DeserializeOwned>(value: serde_json::Value) -> Result<T, Failure> {
    serde_json::from_value(value).map_err(|error| {
        Failure::new(
            &catalog::QUEUE_JOB_FAILED,
            format!("job payload does not match its kind: {error}"),
        )
        .retryable(false)
    })
}

/// Bridge any error the daemon maps into a [`Failure`].
pub fn fail<E: IntoFailure>(error: E) -> Failure {
    error.into_failure()
}

/// A pool handle for handlers that need one without the whole state.
pub type Db = Arc<PgPool>;

#[cfg(test)]
mod tests {
    use super::*;

    /// The schedule this module owns: 2s then 4s, so three attempts span six
    /// seconds rather than three hundred milliseconds.
    #[test]
    fn backoff_grows_with_each_attempt() {
        assert_eq!(backoff(1), Duration::from_secs(2));
        assert_eq!(backoff(2), Duration::from_secs(4));
        assert!(backoff(3) > backoff(2));
        // Clamped, so a job that somehow reached a high count does not sleep
        // for a geological age.
        assert_eq!(backoff(99), backoff(6));
    }

    /// Drained totals are what the e2e test reads; absorbing must not lose a
    /// category.
    #[test]
    fn drain_totals_accumulate_every_outcome() {
        let mut total = Drained::default();
        total.absorb(Drained {
            completed: 2,
            retried: 1,
            failed: 0,
        });
        total.absorb(Drained {
            completed: 1,
            retried: 0,
            failed: 3,
        });
        assert_eq!(total.completed, 3);
        assert_eq!(total.retried, 1);
        assert_eq!(total.failed, 3);
        assert_eq!(total.total(), 7);
    }

    /// A malformed payload is terminal: waiting cannot make it parse.
    #[test]
    fn a_malformed_payload_is_not_retryable() {
        let failure = payload::<crate::roots::ScanFileJob>(serde_json::json!({ "nonsense": true }))
            .expect_err("this payload is not a scan job");
        assert!(!failure.retryable);
        assert_eq!(failure.code, "FS3-E-QUEUE-JOB-FAILED");
    }
}
