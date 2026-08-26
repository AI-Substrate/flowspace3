//! Smart content: the LLM layer, content-addressed by `raw_hash`.
//!
//! Decision D2 in one sentence: enrichment is keyed by the hash of the text it
//! describes, never by an element id. The same function body on forty branches
//! is summarised ONCE; a parser bump re-mints every element row and costs
//! nothing here; a model bump is a new `model_key`, so the old rows survive
//! untouched and rolling back is instant.
//!
//! "Dirty" is therefore not a stored flag. It is the ABSENCE of a row, which is
//! what [`missing_enrichment`] asks for — and why the reconciler in decision D6
//! is self-healing across crashes, model changes and policy changes alike.

use fs3_core::{Summary, content_hash};
use sqlx::Row;

use crate::{PgPool, StoreError};

/// Record the summary of one raw text under one summarising model.
///
/// `text_hash` is derived here with [`fs3_core::content_hash`] — the one hash
/// function in fs3 — rather than by a Postgres expression, so there is exactly
/// one implementation of "the digest of this text" in the system. It is what
/// [`crate::query_embeddings`] follows to get from a summary vector back to the
/// element the summary is about.
///
/// # Errors
/// [`StoreError::Query`], including the `smart_content_tag_band` check when the
/// summary carries no tags or more than five (PRD req 36).
pub async fn put_smart_content(
    pool: &PgPool,
    raw_hash: &str,
    model_key: &str,
    summary: &Summary,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO smart_content (raw_hash, model_key, text, text_hash, tags)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (raw_hash, model_key) DO UPDATE SET
           text      = EXCLUDED.text,
           text_hash = EXCLUDED.text_hash,
           tags      = EXCLUDED.tags",
    )
    .bind(raw_hash)
    .bind(model_key)
    .bind(&summary.text)
    .bind(content_hash(summary.text.as_bytes()))
    .bind(&summary.tags)
    .execute(pool)
    .await?;
    Ok(())
}

/// The summary of one raw text under one model, if it has been made.
///
/// # Errors
/// [`StoreError::Query`] when the statement fails.
pub async fn get_smart_content(
    pool: &PgPool,
    raw_hash: &str,
    model_key: &str,
) -> Result<Option<Summary>, StoreError> {
    let row =
        sqlx::query("SELECT text, tags FROM smart_content WHERE raw_hash = $1 AND model_key = $2")
            .bind(raw_hash)
            .bind(model_key)
            .fetch_optional(pool)
            .await?;

    row.map(|row| {
        Ok(Summary {
            text: row.try_get("text")?,
            tags: row.try_get("tags")?,
            ..Summary::default()
        })
    })
    .transpose()
}

/// One piece of work the reconciler found missing.
///
/// Carries the text itself, not just its hash: the summariser's next step is to
/// read it, and a second query per item to fetch what this row already had
/// would be a round trip bought with nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MissingEnrichment {
    /// The dirtiness key, and the key the resulting summary is stored under.
    pub raw_hash: String,
    /// One element that has this text — an example, not the only one.
    pub address: String,
    /// The blob that element was found in.
    pub blob_sha: String,
    /// The text to summarise.
    pub raw_text: String,
}

/// Elements marked for enrichment that have no summary under `model_key`.
///
/// The decision-D6 reconciler sweep. Deriving the backlog from the schema
/// rather than trusting the queue is what makes a crash, a model change and a
/// policy change all converge without a manual replay.
///
/// Deduplicated by `raw_hash`: forty branches holding the same body are ONE
/// piece of work, and enqueueing it forty times would pay for the same LLM call
/// forty times over.
///
/// # Errors
/// [`StoreError::Query`] when the statement fails.
pub async fn missing_enrichment(
    pool: &PgPool,
    model_key: &str,
    limit: i64,
) -> Result<Vec<MissingEnrichment>, StoreError> {
    let rows = sqlx::query(
        "SELECT DISTINCT ON (e.raw_hash) e.raw_hash, e.address, e.blob_sha, e.raw_text
           FROM elements e
          WHERE e.enrich
            AND NOT EXISTS (
                  SELECT 1 FROM smart_content s
                   WHERE s.raw_hash = e.raw_hash AND s.model_key = $1)
          ORDER BY e.raw_hash, e.id
          LIMIT $2",
    )
    .bind(model_key)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    rows.iter()
        .map(|row| {
            Ok(MissingEnrichment {
                raw_hash: row.try_get("raw_hash")?,
                address: row.try_get("address")?,
                blob_sha: row.try_get("blob_sha")?,
                raw_text: row.try_get("raw_text")?,
            })
        })
        .collect()
}
