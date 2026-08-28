//! Deterministic-document file edges: the inverse index dd cannot answer.
//!
//! Ddoc metadata itself lives on `elements`; this module owns only derived
//! `kind: "file"` edges. Replacement is scoped to one blob/parser pair and is
//! atomic, so a new graph snapshot cannot coexist with stale edges from the
//! previous one.
//!
//! # Composer snap-in recipe
//!
//! No new configuration is required: use the daemon's existing [`crate::PgPool`].
//! After [`crate::upsert_element_tree`] stores a ddoc tree, flatten its file
//! relations into [`DdocFileRef`] values (`element_id: 0`; replacement resolves
//! the database id from `address`) and register the snapshot with:
//!
//! ```ignore
//! let outcome = fs3_store::replace_file_refs(
//!     &state.db,
//!     &blob,
//!     fs3_parsers::PARSER_VERSION,
//!     &file_refs,
//! ).await?;
//! ```
//!
//! Surface every [`FileRefOutcome::unattached`] address as a row finding. A
//! source miss must not hide the rest of a broken-but-indexable document.

use fs3_core::BlobRef;
use sqlx::Row;

use crate::{PgPool, StoreError};

/// One stored file edge and the ddoc row that owns it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DdocFileRef {
    /// Database id of the source row on reads; ignored by replacement.
    pub element_id: i64,
    /// Positional dd address of the source row.
    pub address: String,
    /// Repository-relative ordinary-file target.
    pub path: String,
    /// dd relation spelling, verbatim.
    pub rel: String,
    /// JSONPath into the ddoc source, verbatim.
    pub location: String,
}

/// What replacement attached, and which source rows were not present.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileRefOutcome {
    /// Distinct rows present after this replacement.
    pub attached: usize,
    /// Unresolved source addresses, in input order.
    pub unattached: Vec<String>,
}

/// Atomically replace every file edge for one blob/parser snapshot.
///
/// An unresolved source address is data, not a database failure: resolvable
/// edges are retained and the misses are returned in [`FileRefOutcome`].
/// Resolution never crosses the supplied blob/parser scope and never attaches
/// an edge to a non-row element.
///
/// # Errors
///
/// [`StoreError::Query`] when Postgres refuses the replacement.
pub async fn replace_file_refs(
    pool: &PgPool,
    blob: &BlobRef,
    parser_version: &str,
    refs: &[DdocFileRef],
) -> Result<FileRefOutcome, StoreError> {
    let mut tx = pool.begin().await?;

    sqlx::query(
        "DELETE FROM ddoc_file_refs
          WHERE element_id IN (
                SELECT id FROM elements
                 WHERE blob_sha = $1 AND parser_version = $2)",
    )
    .bind(blob.as_str())
    .bind(parser_version)
    .execute(&mut *tx)
    .await?;

    let addresses: Vec<&str> = refs.iter().map(|item| item.address.as_str()).collect();
    let paths: Vec<&str> = refs.iter().map(|item| item.path.as_str()).collect();
    let rels: Vec<&str> = refs.iter().map(|item| item.rel.as_str()).collect();
    let locations: Vec<&str> = refs.iter().map(|item| item.location.as_str()).collect();

    let row = sqlx::query(
        "WITH input AS (
             SELECT *
               FROM unnest($3::text[], $4::text[], $5::text[], $6::text[])
                    WITH ORDINALITY AS i(address, target_path, rel, location, ordinal)
         ),
         resolved AS (
             SELECT i.*, source.id AS element_id
               FROM input i
               LEFT JOIN LATERAL (
                    SELECT id
                      FROM elements
                     WHERE blob_sha = $1
                       AND parser_version = $2
                       AND kind = 'row'
                       AND address = i.address
                     ORDER BY id
                     LIMIT 1
               ) source ON TRUE
         ),
         inserted AS (
             INSERT INTO ddoc_file_refs (element_id, target_path, rel, location)
             SELECT element_id, target_path, rel, location
               FROM resolved
              WHERE element_id IS NOT NULL
             ON CONFLICT (element_id, target_path, rel, location) DO NOTHING
             RETURNING 1
         )
         SELECT (SELECT count(*) FROM inserted) AS attached,
                COALESCE(
                    array_agg(address ORDER BY ordinal)
                        FILTER (WHERE element_id IS NULL),
                    ARRAY[]::text[]
                ) AS unattached
           FROM resolved",
    )
    .bind(blob.as_str())
    .bind(parser_version)
    .bind(&addresses)
    .bind(&paths)
    .bind(&rels)
    .bind(&locations)
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;

    let attached: i64 = row.try_get("attached")?;
    Ok(FileRefOutcome {
        attached: attached as usize,
        unattached: row.try_get("unattached")?,
    })
}

/// Ddoc rows that reference `target_path`, in deterministic order.
///
/// `repo` limits ownership through live worktree paths. With no matching file
/// edges — including a corpus produced before dd PR #12 — this returns an empty
/// vector, not an error.
///
/// # Errors
///
/// [`StoreError::Query`] when Postgres refuses the lookup.
pub async fn rows_referencing(
    pool: &PgPool,
    repo: Option<&str>,
    target_path: &str,
    limit: i64,
) -> Result<Vec<DdocFileRef>, StoreError> {
    let rows = sqlx::query(
        "SELECT refs.element_id, elements.address, refs.target_path,
                refs.rel, refs.location
           FROM ddoc_file_refs refs
           JOIN elements ON elements.id = refs.element_id
          WHERE refs.target_path = $1
            AND ($2::text IS NULL OR EXISTS (
                 SELECT 1
                   FROM worktree_files files
                   JOIN worktrees ON worktrees.id = files.worktree_id
                   JOIN repos ON repos.id = worktrees.repo_id
                  WHERE files.blob_sha = elements.blob_sha
                    AND repos.identity = $2))
          ORDER BY elements.address, refs.target_path, refs.rel,
                   refs.location, refs.element_id
          LIMIT $3",
    )
    .bind(target_path)
    .bind(repo)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    rows.iter()
        .map(|row| {
            Ok(DdocFileRef {
                element_id: row.try_get("element_id")?,
                address: row.try_get("address")?,
                path: row.try_get("target_path")?,
                rel: row.try_get("rel")?,
                location: row.try_get("location")?,
            })
        })
        .collect()
}
