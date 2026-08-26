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
         RETURNING id, kind, dedupe_key, payload, attempts",
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
        })
    })
    .transpose()
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
