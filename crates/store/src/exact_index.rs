//! The binary copy of every vector that ranks a scope exactly at a fraction
//! of the cost (H1, 2026-10-15; migration 0033).
//!
//! `embeddings_1024_bq` holds `binary_quantize(vector)` for each row of
//! `embeddings_1024`, kept in step by trigger. The exact arm of
//! [`crate::search_elements`] ranks a scope on those 128-byte copies, read
//! index-only, keeps the nearest `fs3.exact_rerank` windows' worth, and
//! re-ranks them by the full vector. It does so only when the database
//! setting `fs3.exact_rank` is `compact` and the table is recorded complete
//! in `search_index_ready`: a half-filled copy would silently drop sources.
//!
//! Rows written before migration 0033 are filled by [`build_exact_index`],
//! run by `flowspace3 doctor exact-index`. It is not a migration: it reads
//! every vector once, a heavy job to schedule, and boot must stay quick.

use sqlx::Row;

use crate::{PgPool, StoreError};

/// The binary copy's table.
pub const EXACT_INDEX_TABLE: &str = "embeddings_1024_bq";

/// Its row in `search_index_ready` once [`build_exact_index`] has filled it.
const READY_NAME: &str = EXACT_INDEX_TABLE;

/// Rows per backfill statement. Each commits on its own, so a long backfill
/// holds no snapshot that would stall vacuum, and an interrupted one resumes
/// where it stopped.
const BACKFILL_BATCH: i64 = 50_000;

/// One embedding model's coverage: vectors, and how many lack their copy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelCoverage {
    pub model_key: String,
    pub vectors: i64,
    pub missing: i64,
}

/// Where the binary copy stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExactIndexState {
    /// Never completed: rows written before migration 0033 may lack a copy.
    NotBuilt { coverage: Vec<ModelCoverage> },
    /// Recorded complete, and every vector of every model has its copy.
    Ready { coverage: Vec<ModelCoverage> },
    /// Recorded complete, yet some vector lacks its copy: something wrote
    /// around the trigger. Search would miss those sources, so
    /// [`guard_exact_index`] clears the mark on sight.
    Drifted { coverage: Vec<ModelCoverage> },
}

impl ExactIndexState {
    /// Every model's coverage.
    #[must_use]
    pub fn coverage(&self) -> &[ModelCoverage] {
        match self {
            Self::NotBuilt { coverage } | Self::Ready { coverage } | Self::Drifted { coverage } => {
                coverage
            }
        }
    }
}

/// Counts each model's vectors and the ones without a copy. Reads both
/// tables in full (about a second at 1.7M vectors), so it belongs to doctor
/// and the build, never to search.
///
/// # Errors
/// [`StoreError::Query`] on database failure.
pub async fn exact_index_coverage(pool: &PgPool) -> Result<Vec<ModelCoverage>, StoreError> {
    let rows = sqlx::query(
        "SELECT e.model_key, count(*)::bigint AS vectors,
                count(*) FILTER (WHERE b.source_hash IS NULL)::bigint AS missing
           FROM embeddings_1024 e
           LEFT JOIN embeddings_1024_bq b
             ON b.source_hash = e.source_hash AND b.source_kind = e.source_kind
            AND b.model_key = e.model_key AND b.chunk_no = e.chunk_no
          GROUP BY e.model_key
          ORDER BY e.model_key",
    )
    .fetch_all(pool)
    .await?;
    rows.iter()
        .map(|row| {
            Ok(ModelCoverage {
                model_key: row.try_get("model_key")?,
                vectors: row.try_get("vectors")?,
                missing: row.try_get("missing")?,
            })
        })
        .collect()
}

/// Reads [`ExactIndexState`] without changing anything.
///
/// # Errors
/// [`StoreError::Query`] on database failure.
pub async fn exact_index_state(pool: &PgPool) -> Result<ExactIndexState, StoreError> {
    let recorded: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM search_index_ready WHERE name = $1)")
            .bind(READY_NAME)
            .fetch_one(pool)
            .await?;
    let coverage = exact_index_coverage(pool).await?;
    let complete = coverage.iter().all(|model| model.missing == 0);
    Ok(match (recorded, complete) {
        (false, _) => ExactIndexState::NotBuilt { coverage },
        (true, true) => ExactIndexState::Ready { coverage },
        (true, false) => ExactIndexState::Drifted { coverage },
    })
}

/// The database's `fs3.exact_rank`, as a new search connection reads it:
/// `compact` or `float` (unset reads as `float`).
///
/// # Errors
/// [`StoreError::Query`] on database failure.
pub async fn exact_rank_setting(pool: &PgPool) -> Result<String, StoreError> {
    let setting: Option<String> =
        sqlx::query_scalar("SELECT NULLIF(current_setting('fs3.exact_rank', true), '')")
            .fetch_one(pool)
            .await?;
    Ok(setting.unwrap_or_else(|| "float".to_string()))
}

/// Removes the copy's ready mark, so search ranks exact scopes on the full
/// vectors until [`build_exact_index`] completes it again.
///
/// # Errors
/// [`StoreError::Query`] on database failure.
pub async fn clear_exact_index_ready(pool: &PgPool) -> Result<(), StoreError> {
    sqlx::query("DELETE FROM search_index_ready WHERE name = $1")
        .bind(READY_NAME)
        .execute(pool)
        .await?;
    Ok(())
}

/// Keeps the ready mark honest: when the copy is marked complete but some
/// vector lacks its copy (a restore of `embeddings_1024` alone, a manual
/// delete, a load with triggers off), clears the mark and returns the
/// coverage that failed. Cheap when the copy is not marked complete: one
/// lookup, no count.
///
/// # Errors
/// [`StoreError::Query`] on database failure.
pub async fn guard_exact_index(pool: &PgPool) -> Result<Option<Vec<ModelCoverage>>, StoreError> {
    let marked: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM search_index_ready WHERE name = $1)")
            .bind(READY_NAME)
            .fetch_one(pool)
            .await?;
    if !marked {
        return Ok(None);
    }
    let coverage = exact_index_coverage(pool).await?;
    if coverage.iter().all(|model| model.missing == 0) {
        return Ok(None);
    }
    clear_exact_index_ready(pool).await?;
    Ok(Some(coverage))
}

/// What [`build_exact_index`] did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExactIndexBuild {
    /// Copies this run wrote (rows that predate the trigger).
    pub filled: i64,
    /// The copy's size on disk, table and key.
    pub bytes: i64,
    /// Every model's coverage afterwards.
    pub coverage: Vec<ModelCoverage>,
    /// Recorded complete: every vector has its copy. False only when vectors
    /// still lack one after the fill, and then nothing was recorded.
    pub ready: bool,
}

/// Fills `embeddings_1024_bq` for every vector that lacks a copy, refreshes
/// its visibility map so the exact arm reads it index-only, and records it
/// complete. On a complete table it only re-checks. Safe beside live writes:
/// the trigger covers every row written meanwhile, and the fill skips any
/// copy that already exists.
///
/// # Errors
/// [`StoreError::Query`] on database failure.
pub async fn build_exact_index(pool: &PgPool) -> Result<ExactIndexBuild, StoreError> {
    let mut filled = 0_i64;
    let mut after: Option<(String, String, i16, String)> = None;
    loop {
        // Walk the vectors' own key in batches: the upper bound of this one
        // is the BACKFILL_BATCH-th key past the last.
        let upper = sqlx::query(
            "SELECT source_hash, source_kind, chunk_no, model_key
               FROM embeddings_1024
              WHERE $1::text IS NULL
                 OR (source_hash, source_kind, chunk_no, model_key) > ($1, $2, $3, $4)
              ORDER BY source_hash, source_kind, chunk_no, model_key
             OFFSET $5 - 1
              LIMIT 1",
        )
        .bind(after.as_ref().map(|key| key.0.as_str()))
        .bind(after.as_ref().map(|key| key.1.as_str()))
        .bind(after.as_ref().map(|key| key.2))
        .bind(after.as_ref().map(|key| key.3.as_str()))
        .bind(BACKFILL_BATCH)
        .fetch_optional(pool)
        .await?
        .map(|row| -> Result<_, StoreError> {
            Ok((
                row.try_get::<String, _>("source_hash")?,
                row.try_get::<String, _>("source_kind")?,
                row.try_get::<i16, _>("chunk_no")?,
                row.try_get::<String, _>("model_key")?,
            ))
        })
        .transpose()?;
        let written = sqlx::query(
            "INSERT INTO embeddings_1024_bq (source_hash, source_kind, model_key, chunk_no, bq)
             SELECT source_hash, source_kind, model_key, chunk_no,
                    binary_quantize(vector)::bit(1024)
               FROM embeddings_1024
              WHERE ($1::text IS NULL
                     OR (source_hash, source_kind, chunk_no, model_key) > ($1, $2, $3, $4))
                AND ($5::text IS NULL
                     OR (source_hash, source_kind, chunk_no, model_key) <= ($5, $6, $7, $8))
             ON CONFLICT DO NOTHING",
        )
        .bind(after.as_ref().map(|key| key.0.as_str()))
        .bind(after.as_ref().map(|key| key.1.as_str()))
        .bind(after.as_ref().map(|key| key.2))
        .bind(after.as_ref().map(|key| key.3.as_str()))
        .bind(upper.as_ref().map(|key| key.0.as_str()))
        .bind(upper.as_ref().map(|key| key.1.as_str()))
        .bind(upper.as_ref().map(|key| key.2))
        .bind(upper.as_ref().map(|key| key.3.as_str()))
        .execute(pool)
        .await?
        .rows_affected();
        filled += i64::try_from(written).unwrap_or(i64::MAX);
        match upper {
            Some(key) => after = Some(key),
            None => break,
        }
    }
    // Index-only scans skip the heap only for pages the visibility map marks
    // all-visible, which a freshly filled table has none of.
    sqlx::query("VACUUM (ANALYZE) embeddings_1024_bq")
        .execute(pool)
        .await?;
    let coverage = exact_index_coverage(pool).await?;
    let ready = coverage.iter().all(|model| model.missing == 0);
    if ready {
        sqlx::query(
            "INSERT INTO search_index_ready (name) VALUES ($1)
             ON CONFLICT (name) DO UPDATE SET ready_at = now()",
        )
        .bind(READY_NAME)
        .execute(pool)
        .await?;
    }
    let bytes: i64 = sqlx::query_scalar("SELECT pg_total_relation_size($1::regclass)")
        .bind(EXACT_INDEX_TABLE)
        .fetch_one(pool)
        .await?;
    Ok(ExactIndexBuild {
        filled,
        bytes,
        coverage,
        ready,
    })
}
