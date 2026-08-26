//! The job backlog: enqueue, claim, settle.
//!
//! One table is the whole queue (decision D1). There is no `dirty_files` table
//! because a dirty file IS a pending `scan_file` job: the watcher firing five
//! times for one save enqueues five times and gets one row, and each re-fire
//! pushes `not_before` further out. The ten-second debounce is that column, not
//! a timer somewhere in the daemon.
//!
//! Claiming uses `FOR UPDATE SKIP LOCKED` (decision D4) — the boring, proven
//! Postgres pattern. Two workers polling at the same instant take two different
//! jobs instead of queueing behind each other, which is what lets an LLM job
//! and an embedding job run at the same time.

use std::time::Duration;

use sqlx::Row;

use crate::{PgPool, StoreError};

/// A claimed job, handed to a worker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Job {
    /// Row id, and the handle for [`complete_job`] / [`fail_job`].
    pub id: i64,
    /// What kind of work this is: `scan_file`, `summarize`, `embed`.
    pub kind: String,
    /// The idempotence key this job was enqueued under.
    pub dedupe_key: String,
    /// The job's own arguments.
    pub payload: serde_json::Value,
    /// How many times this row has been claimed, including now. A worker that
    /// finds a high count is looking at a job that keeps dying.
    pub attempts: i32,
    /// How many times this row was PARKED for provider congestion.
    ///
    /// Separate from `attempts` because congestion is not poison. A parked job
    /// has not failed — the provider asked us to come back later — so parking
    /// returns `attempts` to its pre-claim value and increments this instead.
    /// Folding the two would make a heavily throttled healthy job look exactly
    /// like a flaky one.
    pub parks: i32,
}

/// One `(kind, state)` bucket of the queue.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueueDepth {
    /// The job kind — `scan_file`, `summarize`, `embed`.
    pub kind: String,
    /// `pending`, `running`, `done` or `failed`.
    pub state: String,
    /// How many rows are in this bucket.
    pub depth: i64,
    /// How many of them carry a `last_error` — a retried job that later
    /// succeeded still counts here, which is the point: it is the difference
    /// between "flaky" and "fine".
    pub with_error: i64,
}

/// Put work on the queue, or push an identical piece of work further out.
///
/// The upsert is decision D1's whole mechanism. `dedupe_key` is unique among
/// live (`pending` or `running`) jobs, so a second enqueue of the same key does
/// not add a row — it moves the existing one's deadline, taking the LATER of
/// the two. That is what makes a burst of watcher events collapse into one scan
/// that runs `delay` after the burst STOPS, rather than one scan per event or
/// one that fires while the file is still being written.
///
/// Because the uniqueness is partial, a `done` or `failed` job never blocks the
/// next edit to that file from enqueueing a fresh one.
///
/// Known gap, named rather than hidden: a change arriving while the matching
/// job is already `running` updates that row's deadline but cannot un-run it,
/// so the running pass is the one that settles. The re-scan for that change
/// comes from the decision-D6 reconciler sweep, not from this call.
///
/// # Errors
/// [`StoreError::Query`] when the statement fails.
pub async fn enqueue_job(
    pool: &PgPool,
    kind: &str,
    dedupe_key: &str,
    payload: &serde_json::Value,
    delay: Duration,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO jobs (kind, dedupe_key, payload, not_before)
         VALUES ($1, $2, $3, now() + make_interval(secs => $4))
         ON CONFLICT (dedupe_key) WHERE state IN ('pending', 'running') DO UPDATE SET
           payload    = EXCLUDED.payload,
           not_before = GREATEST(jobs.not_before, EXCLUDED.not_before),
           updated_at = now()",
    )
    .bind(kind)
    .bind(dedupe_key)
    .bind(payload)
    .bind(delay.as_secs_f64())
    .execute(pool)
    .await?;
    Ok(())
}

/// Take one ready job of any of `kinds`, or `None` when there is nothing due.
///
/// `FOR UPDATE SKIP LOCKED` is the point of this query (decision D4): a row
/// another worker is mid-claim on is stepped over rather than waited on, so N
/// workers polling together get N different jobs and none of them block.
///
/// `None` means "nothing ready", not "nothing left" — a job whose `not_before`
/// is still in the future is invisible here, which is how the debounce works.
///
/// # Errors
/// [`StoreError::Query`] when the statement fails.
pub async fn claim_job(pool: &PgPool, kinds: &[&str]) -> Result<Option<Job>, StoreError> {
    let row = sqlx::query(
        "UPDATE jobs SET state = 'running', attempts = attempts + 1, updated_at = now()
          WHERE id = (
                SELECT id FROM jobs
                 WHERE state = 'pending'
                   AND not_before <= now()
                   AND kind = ANY($1)
                 ORDER BY priority DESC, not_before
                 FOR UPDATE SKIP LOCKED
                 LIMIT 1
          )
         RETURNING id, kind, dedupe_key, payload, attempts, parks",
    )
    .bind(kinds)
    .fetch_optional(pool)
    .await?;

    row.map(|row| {
        Ok(Job {
            id: row.try_get("id")?,
            kind: row.try_get("kind")?,
            dedupe_key: row.try_get("dedupe_key")?,
            payload: row.try_get("payload")?,
            attempts: row.try_get("attempts")?,
            parks: row.try_get("parks")?,
        })
    })
    .transpose()
}

/// Claim up to `limit` ready jobs of one kind, in one statement.
///
/// The batching primitive. A `summarize` job is one call per element, but
/// `embed` jobs merge: k jobs' items can ride in ONE wide provider request,
/// which is where the throughput is. Claiming them one at a time would cost k
/// round trips to discover work we are about to merge anyway.
///
/// ONE KIND, not a set. [`claim_job`]'s `kinds` slice exists so a generic
/// worker can take whatever is ready; a caller batching jobs is going to merge
/// their payloads, and payloads are only mergeable within a kind. Accepting a
/// set here would hand back a pile that cannot be batched and make the caller
/// re-sort it.
///
/// Ordering matches [`claim_job`] exactly — `priority DESC, not_before` — so
/// mixing batched and single claimants on one queue cannot starve either.
/// `FOR UPDATE SKIP LOCKED` over the whole `LIMIT` means two workers claiming
/// concurrently take disjoint sets rather than blocking on each other.
///
/// Returns fewer than `limit` when fewer are ready, and empty when none are.
/// Empty means "nothing READY" — a job backing off is invisible here, which is
/// how the debounce works.
///
/// # Errors
/// [`StoreError::Query`] when the statement fails.
pub async fn claim_jobs(pool: &PgPool, kind: &str, limit: i64) -> Result<Vec<Job>, StoreError> {
    if limit <= 0 {
        return Ok(Vec::new());
    }

    let rows = sqlx::query(
        "UPDATE jobs SET state = 'running', attempts = attempts + 1, updated_at = now()
          WHERE id IN (
                SELECT id FROM jobs
                 WHERE state = 'pending'
                   AND not_before <= now()
                   AND kind = $1
                 ORDER BY priority DESC, not_before
                 FOR UPDATE SKIP LOCKED
                 LIMIT $2
          )
         RETURNING id, kind, dedupe_key, payload, attempts, parks",
    )
    .bind(kind)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    rows.iter()
        .map(|row| {
            Ok(Job {
                id: row.try_get("id")?,
                kind: row.try_get("kind")?,
                dedupe_key: row.try_get("dedupe_key")?,
                payload: row.try_get("payload")?,
                attempts: row.try_get("attempts")?,
                parks: row.try_get("parks")?,
            })
        })
        .collect()
}

/// Mark a claimed job finished.
///
/// This is also what frees its `dedupe_key`: the live-jobs unique index stops
/// covering the row, so the next edit to that file enqueues a new job rather
/// than colliding with this one's history.
///
/// # Errors
/// [`StoreError::Query`] when the statement fails.
pub async fn complete_job(pool: &PgPool, id: i64) -> Result<(), StoreError> {
    settle(pool, id, "done", None).await
}

/// Mark a claimed job failed, recording why.
///
/// Terminal, deliberately: there is no retry schedule here. A failed job's work
/// comes back through the decision-D6 reconciler sweep, which derives what is
/// missing from the schema instead of trusting the queue's own memory — and
/// `last_error` stays on the row as the record of what went wrong.
///
/// # Errors
/// [`StoreError::Query`] when the statement fails.
pub async fn fail_job(pool: &PgPool, id: i64, error: &str) -> Result<(), StoreError> {
    settle(pool, id, "failed", Some(error)).await
}

/// Park a claimed job for provider congestion, WITHOUT spending an attempt.
///
/// The difference from [`retry_job`] is the whole point. A retry says "this
/// failed, try again"; a park says "the provider asked us to come back later,
/// and that is not the job's fault". `attempts` is returned to its pre-claim
/// value — [`claim_job`] incremented it on the way in — so a sustained squeeze
/// cannot exhaust jobs that were never broken.
///
/// `parks` counts instead, and it is the caller's bound: a provider that
/// throttles forever would otherwise park a job forever, and nothing would
/// ever notice. Returns the new count so the worker can decide when a park has
/// stopped being congestion and started being a wall.
///
/// # Errors
/// [`StoreError::Query`] when the statement fails.
pub async fn park_job(pool: &PgPool, id: i64, delay: Duration) -> Result<i32, StoreError> {
    let row = sqlx::query(
        "UPDATE jobs
            SET state = 'pending',
                not_before = now() + make_interval(secs => $2),
                attempts = greatest(attempts - 1, 0),
                parks = parks + 1,
                updated_at = now()
          WHERE id = $1
         RETURNING parks",
    )
    .bind(id)
    .bind(delay.as_secs_f64())
    .fetch_one(pool)
    .await?;
    Ok(row.try_get("parks")?)
}

/// Put a claimed job back on the queue, due again after `delay`.
///
/// The store gains a VERB here, not a policy. How many attempts are worth
/// making and how far apart is the worker's decision — the daemon settles it at
/// three attempts with backoff (plan 003) — and this is the one statement that
/// decision needs. Keeping the schedule out of the store is what lets two
/// workers with different appetites share one queue, and it is why
/// [`fail_job`] stays terminal rather than growing a retry mode.
///
/// `attempts` is not touched: [`claim_job`] already incremented it when the row
/// was taken, so a worker deciding whether to retry reads the count it was
/// handed rather than one this function invents.
///
/// `last_error` is recorded even though the row lives on. A job that succeeds
/// on its third attempt still leaves the evidence of the first two, which is
/// the difference between "this is flaky" and "this is fine".
///
/// The row returns to `pending`, so it re-enters the live-dedupe index and a
/// concurrent enqueue of the same key collapses into it rather than racing it.
///
/// # Errors
/// [`StoreError::Query`] when the statement fails.
pub async fn retry_job(
    pool: &PgPool,
    id: i64,
    delay: Duration,
    error: &str,
) -> Result<(), StoreError> {
    sqlx::query(
        "UPDATE jobs
            SET state      = 'pending',
                last_error = $3,
                not_before = now() + make_interval(secs => $2),
                updated_at = now()
          WHERE id = $1",
    )
    .bind(id)
    .bind(delay.as_secs_f64())
    .bind(error)
    .execute(pool)
    .await?;
    Ok(())
}

/// Return every `running` job to `pending`. Returns how many were swept.
///
/// The daemon calls this at BOOT, before its runner starts, and it is sound
/// only there: fs3 has a single writer (PRD req 20), so at that instant no
/// worker can be holding a claimed row — which means every row still marked
/// `running` is one whose worker died mid-job.
///
/// Without the sweep those rows are wedged forever. There is no lease and no
/// heartbeat, so nothing else can ever move them, and `claim_job` only looks at
/// `pending`. Worse, they keep OCCUPYING the live-dedupe index: `scan_file` is
/// keyed by `(worktree, path)`, so a wedged row makes every future `add` or
/// `scan` of that file collapse into it — `enqueue_job`'s `ON CONFLICT` bumps
/// the payload and the deadline but can never change the state. One `SIGKILL`
/// during a large index would leave those files permanently unindexable, and
/// the symptom is silence: the scan reports success and enqueues nothing.
///
/// A lease with an expiry is the general answer and belongs to the daemon plan.
/// This is the whole fix for the crash that actually happens — the process
/// stopping — and it costs one statement at a moment when correctness is free.
///
/// `last_error` records why the row moved, so a job that reappears after a
/// crash is distinguishable from one that was simply slow.
///
/// # Errors
/// [`StoreError::Query`] when the statement fails.
pub async fn requeue_running(pool: &PgPool) -> Result<u64, StoreError> {
    let swept = sqlx::query(
        "UPDATE jobs
            SET state      = 'pending',
                last_error = 'requeued at daemon boot: the worker holding this job did not \
                              finish it',
                updated_at = now()
          WHERE state = 'running'",
    )
    .execute(pool)
    .await?
    .rows_affected();
    Ok(swept)
}

/// How many jobs sit in each state, by kind — what `flowspace3 status` reports.
///
/// Grouped rather than totalled: "142 pending" answers nothing useful, while
/// "142 pending embed, 0 pending scan_file" says the scan finished and the
/// enrichment is the thing to wait for. The zero rows are absent rather than
/// synthesised — a kind fs3 has never run is not a kind at depth zero.
///
/// # Errors
/// [`StoreError::Query`] when the read fails.
/// How many jobs are still to do — `pending` plus `running`, every kind.
///
/// One number, for the per-job streaming line, where the grouped
/// [`queue_depth`] would be both too much detail and too much work to take
/// thousands of times.
///
/// Counted at the source rather than tracked as a counter in the worker,
/// because the backlog GROWS while it drains: each `scan_file` enqueues the
/// `summarize` and `embed` work it discovers. A decrementing counter would
/// march confidently to zero while the real backlog was still climbing, which
/// is worse than no number at all — it reads as "nearly done" at the exact
/// moment it is not.
///
/// `jobs_claim_idx` leads on `state`, so this is an index scan of the live
/// rows and never touches the settled history.
pub async fn jobs_remaining(pool: &PgPool) -> Result<i64, StoreError> {
    let row =
        sqlx::query("SELECT count(*) AS left FROM jobs WHERE state IN ('pending', 'running')")
            .fetch_one(pool)
            .await?;
    Ok(row.try_get("left")?)
}

pub async fn queue_depth(pool: &PgPool) -> Result<Vec<QueueDepth>, StoreError> {
    let rows = sqlx::query(
        "SELECT kind, state, count(*) AS depth,
                count(*) FILTER (WHERE last_error IS NOT NULL) AS with_error
           FROM jobs
          GROUP BY kind, state
          ORDER BY kind, state",
    )
    .fetch_all(pool)
    .await?;

    rows.iter()
        .map(|row| {
            Ok(QueueDepth {
                kind: row.try_get("kind")?,
                state: row.try_get("state")?,
                depth: row.try_get("depth")?,
                with_error: row.try_get("with_error")?,
            })
        })
        .collect()
}

/// The most recent error from a failed job, for a status report that says what
/// went wrong rather than only that something did.
///
/// # Errors
/// [`StoreError::Query`] when the read fails.
pub async fn last_failure(pool: &PgPool) -> Result<Option<(String, String)>, StoreError> {
    let row = sqlx::query(
        "SELECT dedupe_key, last_error FROM jobs
          WHERE state = 'failed' AND last_error IS NOT NULL
          ORDER BY updated_at DESC LIMIT 1",
    )
    .fetch_optional(pool)
    .await?;

    row.map(|row| Ok((row.try_get("dedupe_key")?, row.try_get("last_error")?)))
        .transpose()
}

async fn settle(
    pool: &PgPool,
    id: i64,
    state: &str,
    error: Option<&str>,
) -> Result<(), StoreError> {
    sqlx::query("UPDATE jobs SET state = $2, last_error = $3, updated_at = now() WHERE id = $1")
        .bind(id)
        .bind(state)
        .bind(error)
        .execute(pool)
        .await?;
    Ok(())
}
