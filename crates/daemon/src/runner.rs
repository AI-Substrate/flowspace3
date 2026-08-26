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

/// How often the progress summary is printed while work is in flight.
///
/// Five seconds is slow enough that a long index is a readable handful of
/// lines rather than a scroll, and fast enough that a watcher can tell "moving"
/// from "stuck" without waiting.
const PROGRESS_EVERY: Duration = Duration::from_secs(5);

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
    // Only-when-active, in both directions: nothing is printed while the queue
    // is empty, and the FIRST line after work appears is not delayed by a timer
    // that has been ticking through the idle period.
    let mut last_report: Option<std::time::Instant> = None;

    loop {
        let drained = drain(&state, workers).await;

        if drained.total() == 0 {
            // Print a final summary if work just finished, so a run ends with
            // its own totals rather than trailing off mid-progress.
            if last_report.take().is_some() {
                report_progress(&state, "idle").await;
            }
            tokio::time::sleep(IDLE_POLL).await;
            continue;
        }

        let due = last_report.is_none_or(|at| at.elapsed() >= PROGRESS_EVERY);
        if due {
            report_progress(&state, "working").await;
            last_report = Some(std::time::Instant::now());
        }
    }
}

/// One line saying where the whole index run is up to.
///
/// Derived from the queue rather than from counters this loop keeps, so it is
/// true across restarts and across however many workers are running — a
/// counter in this process would reset on reboot and would not see a sibling's
/// work. The cost is one grouped aggregate every few seconds.
async fn report_progress(state: &AppState, phase: &str) {
    let Ok(rows) = fs3_store::queue_depth(&state.db).await else {
        // A store that cannot answer is already being reported by whatever
        // failed to claim; progress must never add noise to an outage.
        return;
    };

    let count = |kind: &str, state_name: &str| -> i64 {
        rows.iter()
            .filter(|row| row.kind == kind && row.state == state_name)
            .map(|row| row.depth)
            .sum()
    };
    let left = |kind: &str| count(kind, "pending") + count(kind, "running");

    let failed: i64 = rows
        .iter()
        .filter(|row| row.state == "failed")
        .map(|row| row.depth)
        .sum();

    tracing::info!(
        phase,
        scanned = count(SCAN_FILE, "done"),
        scan_left = left(SCAN_FILE),
        summarized = count(SUMMARIZE, "done"),
        summarize_left = left(SUMMARIZE),
        embedded = count(EMBED, "done"),
        embed_left = left(EMBED),
        failed,
        "progress"
    );
}

/// Run one job and settle its row.
async fn settle(state: &AppState, job: Job) -> Drained {
    let id = job.id;
    let attempts = job.attempts;
    let kind = job.kind.clone();
    let key = job.dedupe_key.clone();
    // The human-readable subject, taken from the payload BEFORE dispatch
    // consumes it. A dedupe key is an idempotence token, not a sentence: it
    // says `embed:git:github.com/x:9f2c…`, which tells a watcher nothing about
    // what is happening to their repository.
    let subject = subject_of(&kind, &job.payload);
    let started = std::time::Instant::now();

    match dispatch(state, job).await {
        Ok(()) => {
            if let Err(error) = fs3_store::complete_job(&state.db, id).await {
                tracing::error!(%error, id, "cannot complete job");
            }
            // One line per job, at info, because a healthy index run used to
            // print NOTHING at the default filter — the only calls in this
            // crate were error! and warn!, so a working daemon and a wedged one
            // looked identical from the outside (Jordan, live, 2026-08-26).
            tracing::info!(
                kind = %kind,
                subject = %subject,
                ms = started.elapsed().as_millis() as u64,
                "done"
            );
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

/// The human-readable subject of a job, for one log line.
///
/// Reads only the fields it names — a path, an address, a batch size. Never the
/// payload wholesale: an `embed` payload carries the TEXTS being embedded, and
/// dumping those would put the indexed source code itself into the log, at
/// volume, for every batch.
fn subject_of(kind: &str, payload: &serde_json::Value) -> String {
    let field = |name: &str| payload.get(name).and_then(serde_json::Value::as_str);

    match kind {
        SCAN_FILE => field("path").unwrap_or("?").to_string(),
        SUMMARIZE => payload
            .get("element")
            .and_then(|element| element.get("address"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("?")
            .to_string(),
        EMBED => {
            let count = payload
                .get("items")
                .and_then(serde_json::Value::as_array)
                .map_or(0, Vec::len);
            format!("{count} x {}", field("source").unwrap_or("?"))
        }
        _ => String::new(),
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
