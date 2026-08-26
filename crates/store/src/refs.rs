//! The ref layer: repos, worktrees, and which live path holds which blob.
//!
//! Cheap pointers, per checkout (migration 0003). Nothing expensive hangs off
//! these tables, which is what makes removing a worktree a safe local delete
//! rather than a cascade into re-payable LLM spend (workshop 002, decision D8).
//!
//! The content layer never points here and this never points there. They meet
//! at `blob_sha`, which is a value rather than a foreign key — and that is the
//! whole reason forty branches holding one file share one parse and one
//! summary. The consequence for this module: it is the ONLY place that knows
//! where a blob currently lives, so resolving a content hit back to a path is
//! its job and nobody else's.
//!
//! # Frames
//!
//! `worktrees.root_path` is an absolute host path — where the machine can
//! actually find the checkout — and `worktree_files.path` is relative to it.
//! Registering a subdirectory of a repository is therefore a legal, ordinary
//! thing: the identity is still the repository's (clones and worktrees share
//! derived content), while the paths are in the added root's frame. Blob ids
//! are frame-independent, so content indexed under one frame is never re-parsed
//! or re-enriched when the same bytes turn up under another.

use fs3_core::RepoIdentity;
use sqlx::Row;

use crate::{PgPool, StoreError};

/// A registered worktree, as the store knows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegisteredWorktree {
    /// Row id — the handle [`sync_worktree_files`] writes against, and the
    /// value job payloads carry.
    pub id: i64,
    /// The repository this is a checkout of (PRD req 35).
    pub identity: String,
    /// Absolute host path of the root that was added.
    pub root_path: String,
    /// Branch or ref when known.
    pub ref_name: Option<String>,
    /// How many files this worktree currently maps.
    pub file_count: i64,
}

/// One live path holding a blob.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorktreePath {
    /// The repository the path belongs to.
    pub identity: String,
    /// The worktree root the path is relative to.
    pub root_path: String,
    /// Path relative to [`WorktreePath::root_path`], `/`-separated.
    pub path: String,
}

/// Register a root, returning the worktree row's id.
///
/// Idempotent by `(repo_id, root_path)`: adding the same path twice returns the
/// same id and refreshes the ref name, because `flowspace3 add` on an
/// already-added root is a re-scan request, not a duplicate.
///
/// Both inserts are one transaction. A repo row without its worktree is a
/// repository fs3 believes in but cannot find, and the failure that produced it
/// would be invisible — the next `add` would take the existing repo row and
/// look like it worked.
///
/// # Errors
/// [`StoreError::Query`] when the transaction fails; nothing is written.
pub async fn register_worktree(
    pool: &PgPool,
    identity: &RepoIdentity,
    root_path: &str,
    ref_name: Option<&str>,
) -> Result<i64, StoreError> {
    let mut tx = pool.begin().await?;

    // `DO UPDATE` rather than `DO NOTHING`: the latter returns no row on
    // conflict, which would cost a second round trip on every re-add.
    let repo_id: i64 = sqlx::query(
        "INSERT INTO repos (identity) VALUES ($1)
         ON CONFLICT (identity) DO UPDATE SET identity = EXCLUDED.identity
         RETURNING id",
    )
    .bind(identity.key())
    .fetch_one(&mut *tx)
    .await?
    .try_get("id")?;

    let worktree_id: i64 = sqlx::query(
        "INSERT INTO worktrees (repo_id, root_path, ref_name) VALUES ($1, $2, $3)
         ON CONFLICT (repo_id, root_path) DO UPDATE SET ref_name = EXCLUDED.ref_name
         RETURNING id",
    )
    .bind(repo_id)
    .bind(root_path)
    .bind(ref_name)
    .fetch_one(&mut *tx)
    .await?
    .try_get("id")?;

    tx.commit().await?;
    Ok(worktree_id)
}

/// Replace a worktree's path→blob map with `files`, returning how many paths
/// are no longer there.
///
/// The whole map, not a delta: the caller has just walked the tree, so it knows
/// the complete answer, and reconciling row by row would need a second query to
/// discover what to delete. The delete is scoped to paths absent from this
/// snapshot, so a file removed from disk stops being findable immediately
/// rather than at the next prune.
///
/// Deleting a `worktree_files` row costs nothing derived — that is decision D8
/// working: the element rows, summaries and vectors keyed by the blob survive,
/// so restoring the file (a branch switch, an undo) re-registers a pointer to
/// content that was never thrown away and never has to be paid for twice.
///
/// One transaction, so a scan interrupted halfway leaves the previous map
/// intact rather than a half-updated one that looks authoritative.
///
/// # Errors
/// [`StoreError::Query`] when the transaction fails; nothing is written.
pub async fn sync_worktree_files(
    pool: &PgPool,
    worktree_id: i64,
    files: &[(String, fs3_core::BlobRef)],
) -> Result<u64, StoreError> {
    let mut tx = pool.begin().await?;

    for (path, blob) in files {
        sqlx::query(
            "INSERT INTO worktree_files (worktree_id, path, blob_sha) VALUES ($1, $2, $3)
             ON CONFLICT (worktree_id, path) DO UPDATE SET
               blob_sha  = EXCLUDED.blob_sha,
               last_seen = now()",
        )
        .bind(worktree_id)
        .bind(path)
        .bind(blob.as_str())
        .execute(&mut *tx)
        .await?;
    }

    // `= ANY($2)` with the whole path list rather than a `last_seen` timestamp
    // comparison: a clock-based sweep would delete rows a concurrent scan had
    // just written, and this is exact.
    let paths: Vec<&str> = files.iter().map(|(path, _)| path.as_str()).collect();
    let removed =
        sqlx::query("DELETE FROM worktree_files WHERE worktree_id = $1 AND NOT (path = ANY($2))")
            .bind(worktree_id)
            .bind(&paths)
            .execute(&mut *tx)
            .await?
            .rows_affected();

    tx.commit().await?;
    Ok(removed)
}

/// Every live path currently holding `blob`, across every registered worktree.
///
/// The reverse lookup `worktree_files_blob_sha_idx` exists for. This is how a
/// content hit becomes an answer a human can open: the content layer knows the
/// bytes and the address, and only this table knows where those bytes are right
/// now.
///
/// Ordered by repository then path so a result set is stable between runs.
///
/// # Errors
/// [`StoreError::Query`] when the read fails.
pub async fn worktree_paths_for_blob(
    pool: &PgPool,
    blob: &str,
) -> Result<Vec<WorktreePath>, StoreError> {
    let rows = sqlx::query(
        "SELECT r.identity, w.root_path, f.path
           FROM worktree_files f
           JOIN worktrees w ON w.id = f.worktree_id
           JOIN repos     r ON r.id = w.repo_id
          WHERE f.blob_sha = $1
          ORDER BY r.identity, w.root_path, f.path",
    )
    .bind(blob)
    .fetch_all(pool)
    .await?;

    rows.iter()
        .map(|row| {
            Ok(WorktreePath {
                identity: row.try_get("identity")?,
                root_path: row.try_get("root_path")?,
                path: row.try_get("path")?,
            })
        })
        .collect()
}

/// Every registered worktree with its file count — what `flowspace3 status`
/// reports.
///
/// The count is a correlated aggregate rather than a stored column: a cached
/// counter is one more thing that can be wrong, and "how many files does this
/// root hold" is asked by a human once in a while, not in a loop.
///
/// # Errors
/// [`StoreError::Query`] when the read fails.
pub async fn list_worktrees(pool: &PgPool) -> Result<Vec<RegisteredWorktree>, StoreError> {
    let rows = sqlx::query(
        "SELECT w.id, r.identity, w.root_path, w.ref_name,
                (SELECT count(*) FROM worktree_files f WHERE f.worktree_id = w.id) AS file_count
           FROM worktrees w
           JOIN repos r ON r.id = w.repo_id
          ORDER BY r.identity, w.root_path",
    )
    .fetch_all(pool)
    .await?;

    rows.iter()
        .map(|row| {
            Ok(RegisteredWorktree {
                id: row.try_get("id")?,
                identity: row.try_get("identity")?,
                root_path: row.try_get("root_path")?,
                ref_name: row.try_get("ref_name")?,
                file_count: row.try_get("file_count")?,
            })
        })
        .collect()
}

/// The worktree registered at exactly this root path, if there is one.
///
/// `flowspace3 scan <path>` needs it: re-scanning a root that was never added
/// is a mistake with a clear fix (`add` it first), not a silent no-op.
///
/// # Errors
/// [`StoreError::Query`] when the read fails.
pub async fn find_worktree(
    pool: &PgPool,
    root_path: &str,
) -> Result<Option<RegisteredWorktree>, StoreError> {
    Ok(list_worktrees(pool)
        .await?
        .into_iter()
        .find(|worktree| worktree.root_path == root_path))
}
