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

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use fs3_core::catalog;
use fs3_core::envelope::Failure;
use fs3_core::events::{EventKind, QueueDepth as EventQueueDepth};
use fs3_store::{Job, PgPool};

use tokio::sync::{Semaphore, watch};
use tokio::task::JoinSet;

use crate::answer::IntoFailure;
use crate::batch;
use crate::convo_ingest::INGEST_SESSION;
use crate::enrich::{self, EMBED, SUMMARIZE};
use crate::roots::SCAN_FILE;
use crate::wiring::AppState;

/// Every known job kind.
///
/// Used where the whole queue is the subject, such as boot recovery. Runtime
/// claiming is split across the general, ingest, and embed lanes below.
pub const KINDS: &[&str] = &[SCAN_FILE, INGEST_SESSION, SUMMARIZE, EMBED];

/// The general jobs claimed individually across the general worker pool.
///
/// `scan_file` is local I/O and `summarize` is provider-bound. Conversation
/// ingest deliberately does not share this capacity: a poll must be able to
/// record new turns while enrichment from the previous poll is still queued.
/// Only `embed` batches, and it is drained separately — claiming it here too
/// would have the two paths racing for the same rows.
pub const GENERAL_KINDS: &[&str] = &[SCAN_FILE, SUMMARIZE];

/// How many embed jobs one batched claim takes.
///
/// The token budget does the real limiting; this bounds how much the planner
/// is asked to hold at once, and how many rows a single failed provider call
/// can put back.
pub const EMBED_CLAIM: i64 = 64;

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

/// How many times a job may be parked for congestion before we stop calling it
/// congestion.
///
/// Parking deliberately costs no attempt, which means without a bound a
/// permanently throttled provider would park a job forever and nothing would
/// ever say so. Twenty parks against the schedule below is roughly twenty
/// minutes of patience — long enough to ride out a real quota window, short
/// enough that a misconfigured deployment surfaces the same day.
pub const MAX_PARKS: i32 = 20;

/// How long to park when the service did not say.
///
/// Exponential in the number of parks so far, floored at 5s and capped at 60s,
/// then JITTERED. The jitter is not decoration: k jobs parked by one merged
/// call would otherwise all wake in the same instant and arrive together at a
/// provider that just asked us to slow down.
#[must_use]
pub fn park_delay(parks: i32, retry_after: Option<Duration>) -> Duration {
    if let Some(wait) = retry_after {
        // The service told us. Believe it, and add a little jitter anyway so a
        // merged batch does not return as a thundering herd.
        return wait + jitter(wait.as_millis() as u64 / 8);
    }
    let base = 5u64 << parks.clamp(0, 4);
    let base = base.min(60);
    Duration::from_secs(base) + jitter(base * 250)
}

/// A small pseudo-random spread, seeded from the clock.
///
/// Deliberately not a dependency: this needs to be unpredictable enough that
/// two workers do not collide, not cryptographically random.
fn jitter(span_ms: u64) -> Duration {
    if span_ms == 0 {
        return Duration::ZERO;
    }
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.subsec_nanos() as u64);
    Duration::from_millis(nanos % span_ms)
}

/// How long to wait after finding an empty queue before asking again.
const IDLE_POLL: Duration = Duration::from_millis(250);

/// How often the progress summary is printed while work is in flight.
///
/// Five seconds is slow enough that a long index is a readable handful of
/// lines rather than a scroll, and fast enough that a watcher can tell "moving"
/// from "stuck" without waiting.
const PROGRESS_EVERY: Duration = Duration::from_secs(5);
/// Number of completed summaries that wakes the embed lane immediately.
///
/// Smart embeds arrive one row at a time. Waiting for sixteen restores the
/// provider batch shape without making a busy general lane the only clock.
const EMBED_ACCUMULATE: usize = 16;

/// Maximum time ready embeds wait while general work remains in flight.
const EMBED_MAX_WAIT: Duration = Duration::from_secs(1);

/// The process-wide shutdown phase shared by HTTP and every queue lane.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Shutdown {
    /// Accept requests and dequeue work.
    #[default]
    Running,
    /// Stop dequeueing and finish only work already in flight.
    Draining,
    /// A second signal: cancel remaining work and unwind through cleanup.
    Forced,
}

/// What one drain pass did — the shape the e2e test asserts against.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Drained {
    /// Jobs that completed.
    pub completed: usize,
    /// Jobs put back for another attempt.
    pub retried: usize,
    /// Jobs that failed terminally.
    pub failed: usize,
    /// Jobs parked for provider congestion, costing no attempt.
    pub parked: usize,
}

impl Drained {
    /// Total jobs settled this pass.
    #[must_use]
    pub fn total(&self) -> usize {
        self.completed + self.retried + self.failed + self.parked
    }

    fn absorb(&mut self, other: Drained) {
        self.completed += other.completed;
        self.retried += other.retried;
        self.failed += other.failed;
        self.parked += other.parked;
    }
}
#[derive(Debug, Default)]
struct SummaryReport {
    items: usize,
    outcome: Drained,
    started: Option<std::time::Instant>,
}

impl SummaryReport {
    fn record(&mut self, started: std::time::Instant, outcome: Drained) {
        self.items += outcome.total();
        self.outcome.absorb(outcome);
        self.started = Some(self.started.map_or(started, |current| current.min(started)));
    }

    fn flush(&mut self) {
        if self.items == 0 {
            return;
        }
        let outcome = if self.outcome.failed > 0 {
            "failed"
        } else if self.outcome.retried > 0 {
            "retrying"
        } else if self.outcome.parked > 0 {
            "parked"
        } else {
            "ok"
        };
        let ms = self.started.map_or(0, |started| {
            started.elapsed().as_millis().try_into().unwrap_or(u64::MAX)
        });
        tracing::info!(
            kind = SUMMARIZE,
            items = self.items,
            completed = self.outcome.completed,
            retried = self.outcome.retried,
            failed = self.outcome.failed,
            parked = self.outcome.parked,
            outcome,
            ms,
            "summarize: dispatched group of {} items",
            self.items
        );
        *self = Self::default();
    }
}

/// Run every ready lane until the queue is empty.
///
/// Ingest and general work are drained concurrently. Repeating the pair matters:
/// an ingest can enqueue enrichment after the general pass has already found
/// nothing, and one public drain still promises to consume all work that becomes
/// ready during the pass.
pub async fn drain(state: &AppState, workers: usize) -> Drained {
    let (_shutdown_tx, shutdown) = watch::channel(Shutdown::Running);
    let mut general_shutdown = shutdown.clone();
    let mut ingest_shutdown = shutdown;
    let mut total = Drained::default();

    loop {
        let (general, ingest) = tokio::join!(
            drain_general(state, workers, &mut general_shutdown),
            drain_ingest(state, workers, &mut ingest_shutdown)
        );
        total.absorb(general);
        total.absorb(ingest);
        if general.total() == 0 && ingest.total() == 0 {
            report_progress(state, "idle").await;
            return total;
        }
    }
}

/// Run scan and enrichment jobs until their lanes are empty.
///
/// "Empty" means nothing is *ready*: a job backing off is not ready, so a drain
/// can finish while retries are still pending. Errors are per-job and never
/// abort the pass: one unreadable file must not stop a repository from indexing.
async fn drain_general(
    state: &AppState,
    workers: usize,
    shutdown: &mut watch::Receiver<Shutdown>,
) -> Drained {
    let mut total = Drained::default();
    let mut tasks = JoinSet::new();
    let workers = workers.max(1);
    // The cadence lives HERE, not in the caller's loop. A busy queue never
    // leaves this function — `drain` returns only when nothing is ready — so
    // reporting between drains meant that during a long index run, the one
    // case the summary exists for, it never printed at all.
    let mut last_report: Option<std::time::Instant> = None;
    let mut summarize_lanes: BTreeMap<usize, Arc<Semaphore>> = BTreeMap::new();
    let mut summary_report = SummaryReport::default();

    // Embed work is deliberately NOT claimed after every summary. Sixteen
    // completed summaries fill the fs2-proven shape; the timer bounds
    // staleness while a busy general lane never goes idle; an actually idle
    // lane drains immediately below. Starting due also recovers embed-only
    // work left ready across a daemon restart without waiting a second.
    let mut summaries_waiting = 0usize;
    let mut next_embed = tokio::time::Instant::now();

    loop {
        if *shutdown.borrow() == Shutdown::Forced {
            tasks.shutdown().await;
            return total;
        }

        let embed_due =
            summaries_waiting >= EMBED_ACCUMULATE || tokio::time::Instant::now() >= next_embed;
        let mut embedded = Drained::default();
        if embed_due && *shutdown.borrow() == Shutdown::Running {
            summary_report.flush();
            embedded = drain_embed(state, shutdown).await;
            total.absorb(embedded);
            summaries_waiting = 0;
            next_embed = tokio::time::Instant::now() + EMBED_MAX_WAIT;
        }

        let mut general_exhausted = *shutdown.borrow() != Shutdown::Running;
        while *shutdown.borrow() == Shutdown::Running && tasks.len() < workers {
            match fs3_store::claim_job(&state.db, GENERAL_KINDS).await {
                Ok(Some(job)) => {
                    let state = state.clone();
                    let kind = job.kind.clone();
                    // The SUMMARIZE lane. Held for the whole call so the count
                    // is requests in flight, and clamped per identity by the
                    // summarizer's own ceiling.
                    let lane = (job.kind == SUMMARIZE).then(|| {
                        let summarizer = state.summarizer_for(&summarize_identity(&job.payload));
                        let instance = Arc::as_ptr(summarizer).cast::<()>() as usize;
                        let width = state
                            .config
                            .indexing
                            .summarize_lane
                            .min(summarizer.concurrency_ceiling())
                            .max(1);
                        summarize_lanes
                            .entry(instance)
                            .or_insert_with(|| Arc::new(Semaphore::new(width)))
                            .clone()
                    });
                    tasks.spawn(async move {
                        let _permit = match &lane {
                            Some(lane) => lane.acquire().await.ok(),
                            None => None,
                        };
                        let started = std::time::Instant::now();
                        (kind, started, settle(&state, job).await)
                    });
                }
                Ok(None) => {
                    general_exhausted = true;
                    break;
                }
                Err(error) => {
                    tracing::error!(%error, "cannot claim jobs");
                    general_exhausted = true;
                    break;
                }
            }
        }

        if tasks.is_empty() && general_exhausted {
            summary_report.flush();
            // No general work can add another item: flush a partial batch now
            // rather than making an idle daemon wait for the max-wait clock.
            if !embed_due && *shutdown.borrow() == Shutdown::Running {
                embedded = drain_embed(state, shutdown).await;
                total.absorb(embedded);
                summaries_waiting = 0;
                next_embed = tokio::time::Instant::now() + EMBED_MAX_WAIT;
            }
            if embedded.total() == 0 || *shutdown.borrow() != Shutdown::Running {
                return total;
            }
            if last_report.is_none_or(|at| at.elapsed() >= PROGRESS_EVERY) {
                report_progress(state, "working").await;
                last_report = Some(std::time::Instant::now());
            }
            continue;
        }

        tokio::select! {
            result = tasks.join_next(), if !tasks.is_empty() => {
                if let Some(result) = result {
                    match result {
                        Ok((kind, started, outcome)) => {
                            if kind == SUMMARIZE {
                                summaries_waiting += outcome.completed;
                                summary_report.record(started, outcome);
                            }
                            total.absorb(outcome);
                        }
                        // A panicking handler leaves its row `running` for boot recovery.
                        Err(error) => tracing::error!(%error, "a job handler panicked"),
                    }
                }
            }
            () = tokio::time::sleep_until(next_embed), if *shutdown.borrow() == Shutdown::Running => {}
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() == Shutdown::Forced {
                    tasks.shutdown().await;
                    return total;
                }
            }
        }

        // Reported here rather than inside the per-job branch, because an
        // embed-only workload settles entirely in the batched pass above and
        // spawns no tasks at all — reporting only after `join_next` would give
        // a long embedding run no progress line, which is the exact bug this
        // cadence was moved into the loop to fix.
        if last_report.is_none_or(|at| at.elapsed() >= PROGRESS_EVERY) {
            report_progress(state, "working").await;
            last_report = Some(std::time::Instant::now());
        }
    }
}

/// Run every lane forever for the daemon's background worker.
///
/// The ingest loop is a separate future, not another kind in the general claim
/// set. That distinction is the starvation guarantee: even while a provider call
/// keeps the general drain inside `await`, ingest continues polling Postgres.
pub async fn run_forever(state: AppState, workers: usize) {
    let (_shutdown_tx, shutdown) = watch::channel(Shutdown::Running);
    run_until_shutdown(state, workers, shutdown).await;
}

pub async fn run_until_shutdown(
    state: AppState,
    workers: usize,
    shutdown: watch::Receiver<Shutdown>,
) {
    let reporter_state = state.clone();
    let mut reporter_shutdown = shutdown.clone();
    tokio::join!(
        run_general_forever(state.clone(), workers, shutdown.clone()),
        run_ingest_forever(state, workers, shutdown),
        async move {
            while *reporter_shutdown.borrow() == Shutdown::Running
                && reporter_shutdown.changed().await.is_ok()
            {}
            if *reporter_shutdown.borrow() != Shutdown::Running {
                let in_flight = fs3_store::queue_depth(&reporter_state.db)
                    .await
                    .map(|rows| {
                        rows.into_iter()
                            .filter(|row| row.state == "running")
                            .map(|row| row.depth)
                            .sum::<i64>()
                    })
                    .unwrap_or(0);
                tracing::info!(in_flight, "draining {in_flight} in-flight");
            }
        }
    );
}

async fn run_general_forever(
    state: AppState,
    workers: usize,
    mut shutdown: watch::Receiver<Shutdown>,
) {
    let mut worked = false;

    while *shutdown.borrow() == Shutdown::Running {
        if drain_general(&state, workers, &mut shutdown).await.total() == 0 {
            if std::mem::take(&mut worked) {
                report_progress(&state, "idle").await;
            }
            tokio::select! {
                () = tokio::time::sleep(IDLE_POLL) => {}
                _ = shutdown.changed() => {}
            }
            continue;
        }
        worked = true;
    }
}

async fn run_ingest_forever(
    state: AppState,
    workers: usize,
    mut shutdown: watch::Receiver<Shutdown>,
) {
    while *shutdown.borrow() == Shutdown::Running {
        if drain_ingest(&state, workers, &mut shutdown).await.total() == 0 {
            tokio::select! {
                () = tokio::time::sleep(IDLE_POLL) => {}
                _ = shutdown.changed() => {}
            }
        }
    }
}

/// Drain conversation ingest independently of provider-bound work.
///
/// Different conversations may use the lane concurrently. Two addresses that
/// resolve to one conversation are still serialized by `convo_ingest`'s
/// Postgres advisory lock on the canonical conversation GUID; the queue key only
/// collapses repeated submissions of the same address while one is live.
async fn drain_ingest(
    state: &AppState,
    workers: usize,
    shutdown: &mut watch::Receiver<Shutdown>,
) -> Drained {
    let mut total = Drained::default();
    let mut tasks = JoinSet::new();
    let workers = workers.max(1);

    loop {
        if *shutdown.borrow() == Shutdown::Forced {
            tasks.shutdown().await;
            return total;
        }

        while *shutdown.borrow() == Shutdown::Running && tasks.len() < workers {
            match fs3_store::claim_job(&state.db, &[INGEST_SESSION]).await {
                Ok(Some(job)) => {
                    let state = state.clone();
                    tasks.spawn(async move { settle(&state, job).await });
                }
                Ok(None) => break,
                Err(error) => {
                    tracing::error!(%error, "cannot claim ingest jobs");
                    break;
                }
            }
        }

        if tasks.is_empty() {
            return total;
        }
        tokio::select! {
            result = tasks.join_next() => {
                if let Some(result) = result {
                    match result {
                        Ok(outcome) => total.absorb(outcome),
                        Err(error) => tracing::error!(%error, "an ingest handler panicked"),
                    }
                }
            }
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() == Shutdown::Forced {
                    tasks.shutdown().await;
                    return total;
                }
            }
        }
    }
}

/// Claim a batch of embed jobs and settle them through as few provider calls
/// as the token budget allows.
///
/// # Why a job settles only when EVERY batch carrying it succeeded
///
/// A group of jobs is merged and then cut to the token budget, so one job's
/// items can span several calls. Marking it done after the first call landed
/// would record success for vectors that were never bought — a job that says
/// `done` while half its elements have no row is exactly the silent hole the
/// reconciler cannot see, because the queue's own memory says the work is
/// finished.
async fn drain_embed(state: &AppState, shutdown: &mut watch::Receiver<Shutdown>) -> Drained {
    let jobs = match fs3_store::claim_jobs(&state.db, EMBED, EMBED_CLAIM).await {
        Ok(jobs) if jobs.is_empty() => return Drained::default(),
        Ok(jobs) => jobs,
        Err(error) => {
            tracing::error!(%error, "cannot claim embed jobs");
            return Drained::default();
        }
    };

    let attempts: BTreeMap<i64, i32> = jobs.iter().map(|job| (job.id, job.attempts)).collect();
    let parks: BTreeMap<i64, i32> = jobs.iter().map(|job| (job.id, job.parks)).collect();
    let subjects: BTreeMap<i64, String> = jobs
        .iter()
        .map(|job| (job.id, subject_of(EMBED, &job.payload)))
        .collect();
    let drain_started = std::time::Instant::now();
    let (batches, unreadable) = batch::plan(&jobs);

    let mut total = Drained::default();

    // An unreadable payload is a defect in whoever enqueued it. It can never
    // succeed, so it fails terminally rather than costing three attempts —
    // and terminally here means for good: no boot-time requeue will wake it.
    for bad in unreadable {
        match fs3_store::fail_job(&state.db, bad.job_id, &bad.reason, true).await {
            Ok(()) => {
                emit_failure(
                    state,
                    EMBED,
                    subjects.get(&bad.job_id).map_or("?", String::as_str),
                    &bad.reason,
                    attempts.get(&bad.job_id).copied().unwrap_or(1),
                    true,
                );
            }
            Err(error) => {
                tracing::error!(%error, id = bad.job_id, "cannot fail an unreadable embed job");
            }
        }
        tracing::warn!(id = bad.job_id, kind = EMBED, "{}", bad.reason);
        total.failed += 1;
    }

    // Per job: the failure that settles it, if any batch it rode in failed.
    let mut broken: BTreeMap<i64, Failure> = BTreeMap::new();
    let mut touched: Vec<i64> = Vec::new();

    // THE EMBED LANE. Batches run concurrently, and the width is clamped per
    // PROVIDER INSTANCE — not globally, and not per repo.
    //
    // Not globally: one repo on a single-GPU box declares a ceiling of 1, and
    // taking the minimum across every instance would drop the whole lane to 1
    // because of that one repo.
    //
    // Not per repo either, which is the mistake this comment exists to stop
    // being made again: a ceiling is the PROVIDER's budget, and N repos
    // pointed at the same instance share it. Keying by repo gave five repos on
    // one embedder five concurrent permits against a ceiling of one.
    //
    // The key is therefore the Arc's identity: `wiring` builds one Arc per
    // configured instance and clones it to every repo that selects it, so
    // pointer equality IS instance equality, which is exactly the budget
    // boundary.
    let mut lanes: BTreeMap<usize, Arc<Semaphore>> = BTreeMap::new();
    let mut running = JoinSet::new();

    for one in batches {
        for id in &one.job_ids {
            if !touched.contains(id) {
                touched.push(*id);
            }
        }

        let embedder = state.embedder_for(&one.identity);
        let instance = Arc::as_ptr(embedder).cast::<()>() as usize;
        let width = state
            .config
            .indexing
            .embed_lane
            .min(embedder.concurrency_ceiling())
            .max(1);
        let permits = lanes
            .entry(instance)
            .or_insert_with(|| Arc::new(Semaphore::new(width)))
            .clone();

        let state = state.clone();
        running.spawn(async move {
            // Held for the whole call, so the count is requests IN FLIGHT
            // rather than requests started.
            let _permit = permits.acquire().await;
            let failure = match enrich::source_kind_of(&one.source) {
                Ok(kind) => enrich::embed_items(&state, &one.identity, kind, &one.items)
                    .await
                    .err(),
                Err(failure) => Some(failure),
            };
            (one.job_ids, failure)
        });
    }

    while !running.is_empty() {
        tokio::select! {
            finished = running.join_next() => match finished {
                Some(Ok((job_ids, Some(failure)))) => {
                    for id in job_ids {
                        broken.entry(id).or_insert_with(|| failure.clone());
                    }
                }
                Some(Ok((_, None))) | None => {}
                // A panicking batch leaves its rows `running` for boot recovery.
                Some(Err(error)) => tracing::error!(%error, "an embed batch panicked"),
            },
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() == Shutdown::Forced {
                    running.shutdown().await;
                    return total;
                }
            }
        }
    }

    for id in touched {
        let attempt = attempts.get(&id).copied().unwrap_or(1);
        let subject = subjects.get(&id).map_or("?", String::as_str);
        match broken.remove(&id) {
            None => {
                match fs3_store::complete_job(&state.db, id).await {
                    Ok(()) => {
                        let left = fs3_store::jobs_remaining(&state.db).await.ok();
                        tracing::debug!(kind = EMBED, id, left, "done");
                        if let Some(left) = left {
                            state.emit(EventKind::JobDone {
                                job: EMBED.to_string(),
                                subject: subject.to_string(),
                                ms: drain_started.elapsed().as_millis() as u64,
                                left,
                            });
                        }
                    }
                    Err(error) => {
                        tracing::error!(%error, id, "cannot complete an embed job");
                    }
                }
                total.completed += 1;
            }
            Some(failure) => {
                let parked_before = parks.get(&id).copied().unwrap_or(0);
                let message = format!("{} {}", failure.code, failure.message);
                match verdict(&failure, attempt, parked_before) {
                    Verdict::Park => {
                        let delay = park_delay(parked_before, retry_after_of(&failure));
                        tracing::debug!(
                            id,
                            kind = EMBED,
                            parks = parked_before,
                            wait_s = delay.as_secs(),
                            "parked, no attempt spent: {message}"
                        );
                        match fs3_store::park_job(&state.db, id, delay).await {
                            Ok(_) => {
                                emit_failure(state, EMBED, subject, &message, attempt, false);
                            }
                            Err(error) => {
                                tracing::error!(%error, id, "cannot park an embed job");
                            }
                        }
                        total.parked += 1;
                    }
                    Verdict::Retry => {
                        tracing::debug!(id, kind = EMBED, attempt, retrying = true, "{message}");
                        match fs3_store::retry_job(&state.db, id, backoff(attempt), &message).await
                        {
                            Ok(()) => {
                                emit_failure(state, EMBED, subject, &message, attempt, false);
                            }
                            Err(error) => {
                                tracing::error!(%error, id, "cannot settle a failed embed job");
                            }
                        }
                        total.retried += 1;
                    }
                    Verdict::Fail => {
                        tracing::debug!(id, kind = EMBED, attempt, retrying = false, "{message}");
                        match fs3_store::fail_job(&state.db, id, &message, !failure.retryable).await
                        {
                            Ok(()) => {
                                emit_failure(state, EMBED, subject, &message, attempt, true);
                            }
                            Err(error) => {
                                tracing::error!(%error, id, "cannot settle a failed embed job");
                            }
                        }
                        total.failed += 1;
                    }
                }
            }
        }
    }

    total
}

/// The repo a summarize job belongs to, for lane accounting.
///
/// Falls back to a shared bucket when the payload does not name one: an
/// unattributable job still has to be limited by SOMETHING, and grouping the
/// stragglers together is safer than giving each its own unbounded lane.
fn summarize_identity(payload: &serde_json::Value) -> String {
    payload
        .get("identity")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_string()
}

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
        scan_left = left(SCAN_FILE),
        summarize_left = left(SUMMARIZE),
        embed_left = left(EMBED),
        failed,
        "progress"
    );
    state.emit(EventKind::Queue {
        rows: rows
            .into_iter()
            .map(|row| EventQueueDepth {
                kind: row.kind,
                state: row.state,
                count: row.depth,
            })
            .collect(),
    });
}

fn emit_failure(
    state: &AppState,
    kind: &str,
    subject: &str,
    message: &str,
    attempts: i32,
    terminal: bool,
) {
    state.emit(EventKind::JobFailed {
        job: kind.to_string(),
        subject: subject.to_string(),
        error: message.to_string(),
        attempts: i64::from(attempts),
        terminal,
    });
}

/// Run one job and settle its row.
async fn settle(state: &AppState, job: Job) -> Drained {
    let id = job.id;
    let attempts = job.attempts;
    let parks = job.parks;
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
            } else {
                // Counted after settling, so `left` excludes this event's job.
                let left = fs3_store::jobs_remaining(&state.db).await.ok();
                if kind == SUMMARIZE {
                    tracing::debug!(
                        kind = %kind,
                        subject = %subject,
                        ms = started.elapsed().as_millis() as u64,
                        left,
                        "done"
                    );
                } else {
                    tracing::info!(
                        kind = %kind,
                        subject = %subject,
                        ms = started.elapsed().as_millis() as u64,
                        left,
                        "done"
                    );
                }
                if let Some(left) = left {
                    state.emit(EventKind::JobDone {
                        job: kind.clone(),
                        subject: subject.clone(),
                        ms: started.elapsed().as_millis() as u64,
                        left,
                    });
                }
            }
            Drained {
                completed: 1,
                ..Drained::default()
            }
        }
        Err(failure) => {
            let message = format!("{} {}", failure.code, failure.message);
            match verdict(&failure, attempts, parks) {
                Verdict::Park => {
                    let delay = park_delay(parks, retry_after_of(&failure));
                    if kind == SUMMARIZE {
                        tracing::debug!(
                            id,
                            %kind,
                            parks,
                            wait_s = delay.as_secs(),
                            "parked, no attempt spent: {message}"
                        );
                    } else {
                        tracing::warn!(
                            id,
                            %kind,
                            parks,
                            wait_s = delay.as_secs(),
                            "parked, no attempt spent: {message}"
                        );
                    }
                    match fs3_store::park_job(&state.db, id, delay).await {
                        Ok(_) => {
                            emit_failure(state, &kind, &subject, &message, attempts, false);
                        }
                        Err(error) => tracing::error!(%error, id, "cannot park a job"),
                    }
                    Drained {
                        parked: 1,
                        ..Drained::default()
                    }
                }
                Verdict::Retry => {
                    if kind == SUMMARIZE {
                        tracing::debug!(id, %kind, %key, attempts, retrying = true, "{message}");
                    } else {
                        tracing::warn!(id, %kind, %key, attempts, retrying = true, "{message}");
                    }
                    match fs3_store::retry_job(&state.db, id, backoff(attempts), &message).await {
                        Ok(()) => {
                            emit_failure(state, &kind, &subject, &message, attempts, false);
                        }
                        Err(error) => {
                            tracing::error!(%error, id, "cannot settle a failed job");
                        }
                    }
                    Drained {
                        retried: 1,
                        ..Drained::default()
                    }
                }
                Verdict::Fail => {
                    if kind == SUMMARIZE {
                        tracing::debug!(id, %kind, %key, attempts, retrying = false, "{message}");
                    } else {
                        tracing::warn!(id, %kind, %key, attempts, retrying = false, "{message}");
                    }
                    // The store's terminal bit answers whether this work may
                    // revive after a fix. The event's terminal bit answers
                    // whether this run has stopped trying: every Fail is news.
                    match fs3_store::fail_job(&state.db, id, &message, !failure.retryable).await {
                        Ok(()) => {
                            emit_failure(state, &kind, &subject, &message, attempts, true);
                        }
                        Err(error) => {
                            tracing::error!(%error, id, "cannot settle a failed job");
                        }
                    }
                    Drained {
                        failed: 1,
                        ..Drained::default()
                    }
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
        INGEST_SESSION => crate::convo_ingest::run(state, job.payload).await,
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

/// What to do with a failed job: park it, retry it, or fail it for good.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Congestion. Come back later; spend no attempt.
    Park,
    /// Try again on our own schedule; the attempt is spent.
    Retry,
    /// Terminal.
    Fail,
}

/// Decide a failed job's fate.
///
/// Parking is checked FIRST and independently of `retryable`, because a rate
/// limit is not a failure of the job — the provider is working and busy. Only
/// once congestion has stopped being plausible (`MAX_PARKS`) does a
/// rate-limited job fall back to the ordinary retry ladder.
#[must_use]
pub fn verdict(failure: &Failure, attempts: i32, parks: i32) -> Verdict {
    if failure.code == catalog::PROVIDER_RATE_LIMITED.as_str() && parks < MAX_PARKS {
        return Verdict::Park;
    }
    if failure.retryable && attempts < MAX_ATTEMPTS {
        return Verdict::Retry;
    }
    Verdict::Fail
}

/// The service's own `Retry-After`, if it gave one.
#[must_use]
pub fn retry_after_of(failure: &Failure) -> Option<Duration> {
    failure
        .details
        .get("retry_after_secs")
        .and_then(serde_json::Value::as_f64)
        .map(Duration::from_secs_f64)
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
            parked: 0,
        });
        total.absorb(Drained {
            completed: 1,
            retried: 0,
            failed: 3,
            parked: 0,
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
