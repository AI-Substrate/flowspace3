//! Removing a root, and reclaiming what nothing references any more
//! (PRD req 57).
//!
//! Two operations that look related and are deliberately kept apart:
//!
//! * [`remove_root`] — unregister, in one transaction. Cheap, immediate, and
//!   only ever touches the REF layer plus the removed root's own scan backlog.
//! * [`collect_garbage`] — reclaim content-layer rows nothing references any
//!   more. Slow cadence, batched, and level-triggered, so it reaps residue from
//!   crashes and branch switches as readily as from a removal.
//!
//! # Why removal does not delete content
//!
//! Workshop 002 decision D8: a worktree going away must never cascade into
//! re-payable LLM spend. The content layer is keyed by CONTENT, not by root —
//! forty branches holding one file share one parse and one enrichment — so
//! "this root left" says nothing about whether its content is still needed.
//! Removal therefore unregisters, and GC answers the separate question of what
//! is genuinely unreferenced.
//!
//! Jordan blessed the consequence explicitly (2026-08-27): "its not end of
//! world if some detritus before gc runs". The cadence is the contract.
//!
//! # The three levels, and why there is no cascade to lean on
//!
//! The content layer has NO foreign keys between its levels. Each is
//! independently content-addressed:
//!
//! ```text
//! worktree_files.blob_sha ──> elements.blob_sha
//!                             elements.raw_hash ──> smart_content.raw_hash
//!                                                   smart_content.text_hash ──> embeddings.source_hash ('smart')
//!                             elements.raw_hash ─────────────────────────────> embeddings.source_hash ('raw')
//! ```
//!
//! So "delete what belonged to the removed blob" is WRONG below the first
//! level. `smart_content` is keyed by `raw_hash`, and one raw hash can belong
//! to elements of many different blobs — the same function text in two
//! different files is exactly what content-addressed enrichment exists to
//! exploit. Reaping a summary because ONE of its blobs went away destroys paid
//! LLM output that a still-registered root depends on: D8 violated by the pass
//! written to enforce it.
//!
//! Every level is therefore re-derived from what REMAINS after the level above,
//! which makes sharing survive at every level rather than only the first.

use sqlx::Row;

use crate::{PgPool, StoreError};

/// The job kind that belongs to a root rather than to content.
///
/// `summarize` and `embed` are keyed by blob and element: they may be work for
/// content another registered root still holds, so removal must not touch them
/// (D8). GC reaps the ones that turn out to be unreferenced.
const ROOT_SCOPED_JOB: &str = "scan_file";

/// What [`remove_root`] did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Removal {
    /// The worktree row that went, if the path was registered at all.
    pub worktree_id: Option<i64>,
    /// The repo identity it belonged to.
    pub identity: Option<String>,
    /// How many path→blob mappings went with it.
    pub files: i64,
    /// How many of its queued scans were killed.
    pub jobs_killed: i64,
    /// Whether the repo row went too, because no other worktree of it remained.
    pub repo_removed: bool,
}

impl Removal {
    /// Whether anything was registered at that path.
    #[must_use]
    pub fn was_registered(&self) -> bool {
        self.worktree_id.is_some()
    }
}

/// Unregister the root at `root_path`, killing its queued scans, atomically.
///
/// One transaction, because the half-states are all worse than either end:
/// a worktree with no file map re-scans the world, and a file map with no
/// worktree is unreachable rows that GC would then reap as unreferenced.
///
/// Returns a [`Removal`] with `was_registered` false when nothing was
/// registered there — not an error. `remove` on an unknown path is a question
/// with a true answer, and the caller can say so better than a failure can.
///
/// # What is deliberately NOT deleted
///
/// * `summarize`/`embed` jobs — see [`ROOT_SCOPED_JOB`].
/// * Anything in the content layer — see the module note (D8).
/// * A `scan_file` job in the `done`/`failed` terminal states, which are
///   history rather than work.
///
/// # The running-job race
///
/// [`crate::claim_job`] takes its row lock inside ONE autocommit statement, so
/// a running job's row is not locked for the duration of the job — it is merely
/// marked `running`. This delete therefore does not block on a worker, and
/// needs neither a wait nor a mark-for-death protocol. The only contention is a
/// delete landing inside the claim statement itself, which ordinary row locking
/// settles in microseconds.
///
/// A worker already holding a claimed job settles harmlessly: its `complete`
/// or `fail` updates nothing, and its scan re-reads a worktree that is gone and
/// no-ops.
///
/// # Errors
/// [`StoreError::Query`] when the transaction fails; nothing is written.
pub async fn remove_root(pool: &PgPool, root_path: &str) -> Result<Removal, StoreError> {
    let mut transaction = pool.begin().await?;

    // `FOR UPDATE OF w` serialises two removals of the same root. Without it
    // both transactions read the row before either deletes, and the loser
    // reports `was_registered: true` having deleted nothing — a truthful set of
    // counts wrapped around a misleading headline. With it the loser blocks,
    // re-checks, finds the row gone, and says so.
    //
    // `OF w` rather than a bare `FOR UPDATE`: locking the joined `repos` row
    // as well would make two removals of DIFFERENT worktrees of one repository
    // contend for no reason.
    let Some(found) = sqlx::query(
        "SELECT w.id, w.repo_id, r.identity
           FROM worktrees w
           JOIN repos r ON r.id = w.repo_id
          WHERE w.root_path = $1
            FOR UPDATE OF w",
    )
    .bind(root_path)
    .fetch_optional(&mut *transaction)
    .await?
    else {
        return Ok(Removal::default());
    };

    let worktree_id: i64 = found.try_get("id")?;
    let repo_id: i64 = found.try_get("repo_id")?;
    let identity: String = found.try_get("identity")?;

    // Killed BEFORE the worktree goes, so a crash between the two leaves a
    // registered root with a shortened queue — recoverable by re-scanning —
    // rather than orphaned jobs pointing at a worktree that no longer exists.
    let killed = sqlx::query(
        "DELETE FROM jobs
          WHERE kind = $1
            AND state IN ('pending', 'running')
            AND payload->>'worktree_id' = $2",
    )
    .bind(ROOT_SCOPED_JOB)
    .bind(worktree_id.to_string())
    .execute(&mut *transaction)
    .await?
    .rows_affected();

    // `worktree_files` is ON DELETE CASCADE, so this is counted before it goes.
    let files: i64 =
        sqlx::query_scalar("SELECT count(*) FROM worktree_files WHERE worktree_id = $1")
            .bind(worktree_id)
            .fetch_one(&mut *transaction)
            .await?;

    sqlx::query("DELETE FROM worktrees WHERE id = $1")
        .bind(worktree_id)
        .execute(&mut *transaction)
        .await?;

    // The repo row goes only when it has no other checkout. Two worktrees of
    // one repository are the ordinary case (a branch in a second directory),
    // and taking the repo out from under the survivor would orphan it.
    let repo_removed = sqlx::query("DELETE FROM repos WHERE id = $1 AND NOT EXISTS (SELECT 1 FROM worktrees WHERE repo_id = $1)")
        .bind(repo_id)
        .execute(&mut *transaction)
        .await?
        .rows_affected()
        > 0;

    transaction.commit().await?;

    Ok(Removal {
        worktree_id: Some(worktree_id),
        identity: Some(identity),
        files,
        jobs_killed: i64::try_from(killed).unwrap_or(i64::MAX),
        repo_removed,
    })
}

/// Whether this worktree is still registered.
///
/// The check a scan worker runs before it writes: a job claimed just before a
/// removal must no-op rather than fail on the foreign key, and "is my desired
/// state still desired" is the level-triggered way to ask.
///
/// # Errors
/// [`StoreError::Query`] when the read fails.
pub async fn worktree_exists(pool: &PgPool, worktree_id: i64) -> Result<bool, StoreError> {
    Ok(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM worktrees WHERE id = $1")
            .bind(worktree_id)
            .fetch_one(pool)
            .await?
            > 0,
    )
}

/// The reference predicate, written once because it appears in FIVE statements
/// and a copy that drifts deletes paid LLM output for content that is still
/// live.
///
/// Content is held while a registered worktree maps the blob an element came
/// from, OR while a stored turn carries it. The second leg is what makes a
/// conversation a ROOT of reference (workshop 005): an imported conversation
/// has no worktree and never will, so without it the very first GC pass
/// reclaims every turn element and every summary and vector the import paid
/// for — silently, because an empty search result looks like "no match".
///
/// A macro rather than a `const`: `concat!` splices literals at compile time
/// and cannot take a `const`, and building these statements at runtime would
/// trade a compile-time guarantee for a `LazyLock`.
macro_rules! held_by_a_live_root {
    () => {
        "(EXISTS (SELECT 1 FROM worktree_files wf WHERE wf.blob_sha = e.blob_sha)
             OR EXISTS (SELECT 1 FROM turns t WHERE t.blob_sha = e.blob_sha))"
    };
}

/// Whether `raw_hash` still belongs to any element of a blob a registered
/// worktree holds, or that a stored turn carries.
///
/// The guard at the point of spend, and deliberately the SAME predicate GC
/// uses at level two. `summarize` and `embed` are keyed by `raw_hash`, not by
/// blob, so asking "is this blob still around" would be the wrong question:
/// one raw hash can belong to elements of many blobs, and it survives while
/// any ONE of them is still referenced.
///
/// Without it, a job queued for a root that has since been removed pays a
/// provider for content nobody can ever search. GC reaps such jobs on its own
/// cadence, but the queue drains faster than GC runs — and a job already
/// CLAIMED when the removal landed is one GC can never reach.
///
/// # Errors
/// [`StoreError::Query`] when the read fails.
pub async fn raw_hash_is_referenced(pool: &PgPool, raw_hash: &str) -> Result<bool, StoreError> {
    Ok(sqlx::query_scalar::<_, i64>(concat!(
        "SELECT count(*)
           FROM elements e
          WHERE e.raw_hash = $1
            AND ",
        held_by_a_live_root!()
    ))
    .bind(raw_hash)
    .fetch_one(pool)
    .await?
        > 0)
}

/// Which of these embedding source hashes still belong to content a live root
/// holds, or a stored turn carries.
///
/// [`raw_hash_is_referenced`] answers this one text at a time, which is the
/// right shape for `summarize` — one job, one text, one guard. An `embed` job
/// carries a BATCH, so asking per item would be sixteen round trips to decide
/// one provider call; this asks once and hands back the survivors.
///
/// The two kinds live in different spaces and both have to be asked about,
/// exactly as the level-0 collector does. A `raw` hash IS an element's
/// `raw_hash`. A `smart` hash is a summary's `text_hash`, which reaches an
/// element only through the summary row — so asking the first question of a
/// smart batch would answer "unreferenced" for every summary vector still
/// waiting to be bought, and the guard would delete the index instead of
/// protecting the bill.
///
/// # Errors
/// [`StoreError::Query`] when the read fails.
pub async fn referenced_source_hashes(
    pool: &PgPool,
    source_kind: crate::SourceKind,
    hashes: &[&str],
) -> Result<std::collections::HashSet<String>, StoreError> {
    if hashes.is_empty() {
        return Ok(std::collections::HashSet::new());
    }

    let sql = match source_kind {
        crate::SourceKind::Raw => concat!(
            "SELECT DISTINCT e.raw_hash AS source_hash
               FROM elements e
              WHERE e.raw_hash = ANY($1)
                AND ",
            held_by_a_live_root!()
        ),
        crate::SourceKind::Smart => concat!(
            "SELECT DISTINCT s.text_hash AS source_hash
               FROM smart_content s
               JOIN elements e ON e.raw_hash = s.raw_hash
              WHERE s.text_hash = ANY($1)
                AND ",
            held_by_a_live_root!()
        ),
    };

    // ANY($1) over one indexed read, whatever the batch size — the same shape
    // the dedupe pre-check uses, so the guard adds one round trip to an embed
    // job rather than one per item.
    let owned: Vec<String> = hashes.iter().map(|hash| (*hash).to_string()).collect();
    let rows = sqlx::query(sql).bind(&owned).fetch_all(pool).await?;

    rows.iter()
        .map(|row| Ok(row.try_get("source_hash")?))
        .collect()
}

/// What one GC pass reclaimed, or could.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Reclaimed {
    /// Queued enrichment for content nothing holds any more.
    pub jobs: i64,
    /// Parse trees for blobs no worktree maps.
    pub elements: i64,
    /// Summaries for text no remaining element carries.
    pub summaries: i64,
    /// Vectors whose source no longer exists.
    pub embeddings: i64,
}

impl Reclaimed {
    /// Whether there was anything to do.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Total rows, for a one-line report.
    #[must_use]
    pub fn total(&self) -> i64 {
        self.jobs + self.elements + self.summaries + self.embeddings
    }
}

/// How many rows one statement deletes before coming up for air.
///
/// Batched because a first GC over a long-neglected database could otherwise
/// be one transaction holding locks over millions of rows — the pass would
/// block writers and, if it failed near the end, roll back all of it and try
/// the same doomed thing next time. Small batches make progress durable and
/// interruptible: a pass that dies halfway has still reclaimed half.
const BATCH: i64 = 500;

/// Count what is collectable AT THIS INSTANT, changing nothing.
///
/// A **floor, not a forecast.** The levels cascade: a summary only becomes
/// collectable once the elements carrying its raw hash have actually gone, so
/// this under-reports whatever a real pass will reach. Simulating the cascade
/// read-only would mean modelling three deletes inside one query — a second
/// implementation of the collector, written in SQL, free to drift from the
/// one that runs.
///
/// So the number stays honest-and-low. `flowspace3 remove` says "reclaimable
/// by GC" rather than promising a total, and `flowspace3 gc` reports what it
/// actually did.
///
/// # Errors
/// [`StoreError::Query`] when a read fails.
pub async fn reclaimable(pool: &PgPool) -> Result<Reclaimed, StoreError> {
    Ok(Reclaimed {
        jobs: count(pool, UNREFERENCED_JOBS).await?,
        elements: count(pool, UNREFERENCED_ELEMENTS).await?,
        summaries: count(pool, UNREFERENCED_SUMMARIES).await?,
        embeddings: count(pool, UNREFERENCED_EMBEDDINGS).await?,
    })
}

/// Reclaim everything nothing references any more.
///
/// **Level-triggered, and that is the whole safety argument.** Each level
/// re-derives its unreferenced set from what REMAINS after the level above,
/// rather than from what was just deleted. Two consequences:
///
/// * Sharing survives at every level. A blob two repos hold survives the
///   removal of one; a raw hash carried by elements of two DIFFERENT blobs
///   survives the collection of one of them. The second is the one that
///   matters — enrichment is keyed by raw hash, so getting it wrong destroys
///   paid LLM output for a root that is still registered.
/// * It reaps residue nobody removed on purpose: a crash mid-scan, a branch
///   switch, an old removal from before this code existed. GC is not "the
///   remove verb's cleanup"; it is a statement about the whole database that
///   happens to be true after a removal too.
///
/// Ordering is top-down and each level runs to exhaustion, so one pass reaches
/// a fixed point rather than needing three cadences to finish.
///
/// # Errors
/// [`StoreError::Query`] when a statement fails. Batches already committed
/// stay committed — partial progress is the point.
pub async fn collect_garbage(pool: &PgPool) -> Result<Reclaimed, StoreError> {
    Ok(Reclaimed {
        jobs: drain(pool, DELETE_UNREFERENCED_JOBS).await?,
        elements: drain(pool, DELETE_UNREFERENCED_ELEMENTS).await?,
        summaries: drain(pool, DELETE_UNREFERENCED_SUMMARIES).await?,
        embeddings: drain(pool, DELETE_UNREFERENCED_EMBEDDINGS).await?,
    })
}

/// Run a batched delete until it stops finding anything.
async fn drain(pool: &PgPool, statement: &str) -> Result<i64, StoreError> {
    let mut total = 0;
    loop {
        let removed = sqlx::query(statement)
            .bind(BATCH)
            .execute(pool)
            .await?
            .rows_affected();
        total += i64::try_from(removed).unwrap_or(i64::MAX);
        if removed == 0 {
            return Ok(total);
        }
    }
}

async fn count(pool: &PgPool, predicate: &str) -> Result<i64, StoreError> {
    Ok(sqlx::query_scalar(predicate).fetch_one(pool).await?)
}

/// Whether a queued enrichment job is still work worth doing.
///
/// The two job kinds do NOT carry the same payload, and reading only one shape
/// is how a level-0 sweep silently deletes live work. A `summarize` job names
/// one `raw_hash`. An `embed` job carries a BATCH, as `items`, because an
/// embeddings API charges per call as much as per token — it has no `raw_hash`
/// field at all, so a predicate asking for one reads NULL, concludes nothing
/// references it, and reaps it.
///
/// The consequence is the silent kind: elements stay, summaries stay, nothing
/// fails and nothing logs, but the vectors are never bought — so the content is
/// permanently invisible to semantic search while `status` looks healthy.
///
/// The item hashes live in two different spaces, and both have to be asked
/// about. A `raw` batch's hashes are element `raw_hash`es. A `smart` batch's
/// are `smart_content.text_hash`es — the hash of the SUMMARY text, which is
/// what lets a smart hit resolve back to what it describes — so those reach an
/// element only through the summary row. Asking only the first would reap
/// every summary vector still waiting to be bought.
macro_rules! job_is_still_wanted {
    () => {
        concat!(
            "(EXISTS (
             SELECT 1 FROM elements e
              WHERE e.raw_hash = j.payload->>'raw_hash'
                AND ",
            held_by_a_live_root!(),
            ")
       OR EXISTS (
             SELECT 1
               FROM jsonb_array_elements(coalesce(j.payload->'items', '[]'::jsonb)) AS item
               JOIN elements e ON e.raw_hash = item->>0
              WHERE ",
            held_by_a_live_root!(),
            ")
       OR EXISTS (
             SELECT 1
               FROM jsonb_array_elements(coalesce(j.payload->'items', '[]'::jsonb)) AS item
               JOIN smart_content s ON s.text_hash = item->>0
               JOIN elements e      ON e.raw_hash = s.raw_hash
              WHERE ",
            held_by_a_live_root!(),
            "))"
        )
    };
}

/// Level 0 — enrichment queued for content nothing holds.
///
/// Pending only: a `running` job is a worker's business, and it has its own
/// check at the point of spend ([`raw_hash_is_referenced`]).
const UNREFERENCED_JOBS: &str = concat!(
    "SELECT count(*) FROM jobs j
  WHERE j.kind IN ('summarize', 'embed')
    AND j.state = 'pending'
    AND NOT ",
    job_is_still_wanted!()
);

const DELETE_UNREFERENCED_JOBS: &str = concat!(
    "DELETE FROM jobs
  WHERE ctid IN (
        SELECT j.ctid FROM jobs j
         WHERE j.kind IN ('summarize', 'embed')
           AND j.state = 'pending'
           AND NOT ",
    job_is_still_wanted!(),
    "
         LIMIT $1)"
);

/// Level 1 — parse trees for blobs no worktree maps and no turn carries.
const UNREFERENCED_ELEMENTS: &str = concat!(
    "SELECT count(*) FROM elements e
  WHERE NOT ",
    held_by_a_live_root!()
);

/// Children go with their parents by `ON DELETE CASCADE`, so a batch may
/// remove more rows than it names — which is why the loop trusts
/// `rows_affected` rather than assuming `BATCH`.
const DELETE_UNREFERENCED_ELEMENTS: &str = concat!(
    "DELETE FROM elements
  WHERE ctid IN (
        SELECT e.ctid FROM elements e
         WHERE NOT ",
    held_by_a_live_root!(),
    "
         LIMIT $1)"
);

/// Level 2 — summaries no REMAINING element still points at.
///
/// Keyed by `raw_hash`, so this is the level where sharing across different
/// blobs lives or dies.
const UNREFERENCED_SUMMARIES: &str = "SELECT count(*) FROM smart_content s
  WHERE NOT EXISTS (SELECT 1 FROM elements e WHERE e.raw_hash = s.raw_hash)";

const DELETE_UNREFERENCED_SUMMARIES: &str = "DELETE FROM smart_content
  WHERE ctid IN (
        SELECT s.ctid FROM smart_content s
         WHERE NOT EXISTS (SELECT 1 FROM elements e WHERE e.raw_hash = s.raw_hash)
         LIMIT $1)";

/// Level 3 — vectors whose source is gone, on either side of the union.
///
/// `source_kind` decides which table to ask: `raw` vectors hang off an
/// element's `raw_hash`, `smart` ones off a summary's `text_hash`. Asking only
/// one would reap half the index.
const UNREFERENCED_EMBEDDINGS: &str = "SELECT count(*) FROM embeddings_1024 v
  WHERE NOT EXISTS (
          SELECT 1 FROM elements e
           WHERE v.source_kind = 'raw' AND e.raw_hash = v.source_hash)
    AND NOT EXISTS (
          SELECT 1 FROM smart_content s
           WHERE v.source_kind = 'smart' AND s.text_hash = v.source_hash)";

const DELETE_UNREFERENCED_EMBEDDINGS: &str = "DELETE FROM embeddings_1024
  WHERE ctid IN (
        SELECT v.ctid FROM embeddings_1024 v
         WHERE NOT EXISTS (
                 SELECT 1 FROM elements e
                  WHERE v.source_kind = 'raw' AND e.raw_hash = v.source_hash)
           AND NOT EXISTS (
                 SELECT 1 FROM smart_content s
                  WHERE v.source_kind = 'smart' AND s.text_hash = v.source_hash)
         LIMIT $1)";
