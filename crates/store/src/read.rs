//! The read surface's store queries: where a path lives, and what parsed it.
//!
//! Search asks "what is nearest"; `get` and `tree` ask "what is AT this
//! address". That is a different set of questions, and they all bottom out in
//! the ref layer — the only place that knows which live path holds which blob
//! (migration 0003) — before crossing to the content layer at `blob_sha`.
//!
//! Nothing here assembles an element tree: [`crate::get_elements`] already does
//! that for a `(blob, parser_version)` pair, and a second tree-builder written
//! in SQL would be free to disagree with the one the scan flow proves every
//! day. These functions hand back the keys that make that call possible.

use fs3_core::ports::Summary;
use sqlx::Row;

use crate::smart::summary_from_row;
use crate::{PgPool, StoreError};

/// One live path holding one blob, with the repository it belongs to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexedFile {
    /// The repository identity (PRD req 35).
    pub identity: String,
    /// The worktree root the path is relative to.
    pub root_path: String,
    /// Path relative to [`IndexedFile::root_path`], `/`-separated.
    pub path: String,
    /// The content key of the bytes at that path.
    pub blob_sha: String,
}

/// Every repository identity in the index, longest first.
///
/// Longest first because that is the order an address is resolved in: an
/// identity contains `/`, so the boundary between repo and path can only be
/// found by preferring the most specific identity that matches
/// (`fs3_core::ElementAddress::split`).
///
/// # Errors
/// [`StoreError::Query`] when the read fails.
pub async fn repo_identities(pool: &PgPool) -> Result<Vec<String>, StoreError> {
    let rows = sqlx::query("SELECT identity FROM repos ORDER BY length(identity) DESC, identity")
        .fetch_all(pool)
        .await?;
    rows.iter().map(|row| Ok(row.try_get("identity")?)).collect()
}

/// Every indexed file at exactly `path`, optionally within one repository.
///
/// More than one row is a real answer, not a defect: the same repo-relative
/// path exists in every checkout of a repository, and in unrelated
/// repositories too. The caller decides whether that is ambiguity (a `get`
/// with no `--repo`) or breadth (a `tree`).
///
/// # Errors
/// [`StoreError::Query`] when the read fails.
pub async fn files_at_path(
    pool: &PgPool,
    repo: Option<&str>,
    path: &str,
) -> Result<Vec<IndexedFile>, StoreError> {
    let rows = sqlx::query(
        "SELECT DISTINCT r.identity, w.root_path, f.path, f.blob_sha
           FROM worktree_files f
           JOIN worktrees w ON w.id = f.worktree_id
           JOIN repos r     ON r.id = w.repo_id
          WHERE f.path = $1
            AND ($2::text IS NULL OR r.identity = $2)
          ORDER BY r.identity, w.root_path",
    )
    .bind(path)
    .bind(repo)
    .fetch_all(pool)
    .await?;

    rows.iter().map(indexed_file).collect()
}

/// Indexed files under `prefix`, optionally within one repository.
///
/// `prefix` is a path prefix, not a glob: `tree` browses structure, and a
/// caller who wants pattern matching is describing a `search --path`. An empty
/// or absent prefix is the whole repository.
///
/// The `limit` is the caller's ceiling on how much structure to render, and it
/// is applied in SQL rather than after the fetch so a repository with a hundred
/// thousand files costs the same as one with ten.
///
/// # Errors
/// [`StoreError::Query`] when the read fails.
pub async fn files_under(
    pool: &PgPool,
    repo: Option<&str>,
    prefix: Option<&str>,
    limit: i64,
) -> Result<Vec<IndexedFile>, StoreError> {
    // `left(path, n) = prefix` rather than `LIKE prefix || '%'`: a path is data
    // and may legitimately contain `_` or `%`, which LIKE would silently read
    // as wildcards.
    let rows = sqlx::query(
        "SELECT DISTINCT r.identity, w.root_path, f.path, f.blob_sha
           FROM worktree_files f
           JOIN worktrees w ON w.id = f.worktree_id
           JOIN repos r     ON r.id = w.repo_id
          WHERE ($2::text IS NULL OR r.identity = $2)
            AND ($3::text IS NULL
                 OR f.path = $3
                 OR left(f.path, length($3) + 1) = $3 || '/')
          ORDER BY r.identity, f.path
          LIMIT $1",
    )
    .bind(limit)
    .bind(repo)
    .bind(prefix.filter(|value| !value.is_empty()))
    .fetch_all(pool)
    .await?;

    rows.iter().map(indexed_file).collect()
}

/// How many indexed files a repository holds, and under a prefix.
///
/// `tree` reports a total beside a truncated listing, because "47 of 12,904"
/// is navigable and "47" pretending to be everything is a lie a caller cannot
/// detect.
///
/// # Errors
/// [`StoreError::Query`] when the read fails.
pub async fn count_files_under(
    pool: &PgPool,
    repo: Option<&str>,
    prefix: Option<&str>,
) -> Result<i64, StoreError> {
    let count: i64 = sqlx::query_scalar(
        "SELECT count(DISTINCT (r.identity, f.path))
           FROM worktree_files f
           JOIN worktrees w ON w.id = f.worktree_id
           JOIN repos r     ON r.id = w.repo_id
          WHERE ($1::text IS NULL OR r.identity = $1)
            AND ($2::text IS NULL
                 OR f.path = $2
                 OR left(f.path, length($2) + 1) = $2 || '/')",
    )
    .bind(repo)
    .bind(prefix.filter(|value| !value.is_empty()))
    .fetch_one(pool)
    .await?;
    Ok(count)
}

/// The parser versions that have produced elements for `blob`, most recently
/// written first.
///
/// Elements are keyed by `(blob_sha, parser_version)`, so a parser bump leaves
/// the previous version's rows in place until a re-scan replaces them. A reader
/// that only ever asked for the CURRENT version would answer "not found" for
/// every address in the index during that window — a silent cliff after an
/// upgrade. The caller prefers the current version and falls back to the most
/// recent stored one, saying which it used.
///
/// # Errors
/// [`StoreError::Query`] when the read fails.
pub async fn parser_versions_for_blob(
    pool: &PgPool,
    blob: &str,
) -> Result<Vec<String>, StoreError> {
    let rows = sqlx::query(
        "SELECT parser_version
           FROM elements
          WHERE blob_sha = $1
          GROUP BY parser_version
          ORDER BY max(id) DESC",
    )
    .bind(blob)
    .fetch_all(pool)
    .await?;
    rows.iter()
        .map(|row| Ok(row.try_get("parser_version")?))
        .collect()
}

/// The most recent summary of `raw_hash` under any summarising model.
///
/// `get` shows the summary that exists rather than demanding one from a model
/// the caller has to name: enrichment is keyed by `raw_hash` and a model key
/// (workshop 002 D2), and after a model bump both rows are real. The newest is
/// the one that describes the current understanding of the text.
///
/// # Errors
/// [`StoreError::Query`] when the read fails.
pub async fn latest_summary(
    pool: &PgPool,
    raw_hash: &str,
) -> Result<Option<Summary>, StoreError> {
    let row = sqlx::query(
        "SELECT text, tags, extras
           FROM smart_content
          WHERE raw_hash = $1
          ORDER BY created_at DESC, model_key
          LIMIT 1",
    )
    .bind(raw_hash)
    .fetch_optional(pool)
    .await?;

    row.as_ref().map(summary_from_row).transpose()
}

/// The registered worktree `path` is inside, if any.
///
/// The ANCESTOR match [`crate::find_worktree`] deliberately is not: `scan`
/// needs the root registered at exactly that path, while scoping a query needs
/// the root a caller is standing SOMEWHERE inside. Longest root wins, so a
/// nested registration (a subdirectory added on its own) beats the outer one —
/// the more specific root is the one the caller is actually in.
///
/// The boundary test is `left(path, len+1) = root || '/'` rather than a `LIKE`,
/// so a root path containing `_` or `%` cannot behave as a wildcard.
///
/// # Errors
/// [`StoreError::Query`] when the read fails.
pub async fn worktree_containing(
    pool: &PgPool,
    path: &str,
) -> Result<Option<crate::refs::RegisteredWorktree>, StoreError> {
    let row = sqlx::query(
        "SELECT w.id, r.identity, w.root_path, w.ref_name,
                (SELECT count(*) FROM worktree_files f WHERE f.worktree_id = w.id) AS file_count
           FROM worktrees w
           JOIN repos r ON r.id = w.repo_id
          WHERE $1 = w.root_path
             OR left($1, length(w.root_path) + 1) = w.root_path || '/'
          ORDER BY length(w.root_path) DESC
          LIMIT 1",
    )
    .bind(path)
    .fetch_optional(pool)
    .await?;

    row.map(|row| {
        Ok(crate::refs::RegisteredWorktree {
            id: row.try_get("id")?,
            identity: row.try_get("identity")?,
            root_path: row.try_get("root_path")?,
            ref_name: row.try_get("ref_name")?,
            file_count: row.try_get("file_count")?,
        })
    })
    .transpose()
}

fn indexed_file(row: &sqlx::postgres::PgRow) -> Result<IndexedFile, StoreError> {
    Ok(IndexedFile {
        identity: row.try_get("identity")?,
        root_path: row.try_get("root_path")?,
        path: row.try_get("path")?,
        blob_sha: row.try_get("blob_sha")?,
    })
}
