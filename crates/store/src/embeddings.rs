//! Embeddings: vectors, and the similarity query that reads them.
//!
//! Decision D3: one table per vector width. An HNSW index needs a typed
//! dimension, so a single untyped `vector` column could not be indexed at all
//! and every similarity query would be a sequential scan. `embeddings_1024` is
//! the only width configured today; another model's width arrives as another
//! migration, owned by the daemon (the single writer).
//!
//! A vector's `source_hash` is either an element's `raw_hash` (a raw-content
//! vector) or a summary's `text_hash` (a smart vector), and `source_kind` says
//! which. That pair is what [`query_embeddings`] follows back to the element a
//! hit is about.

use fs3_core::Element;
use pgvector::Vector;
use sqlx::Row;

use crate::elements::kind_from_str;
use crate::{PgPool, StoreError};

/// The vector width `embeddings_1024` holds.
pub const EMBEDDING_DIMENSIONS: usize = 1024;

/// Which text a vector was made from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SourceKind {
    /// The element's own source slice.
    Raw,
    /// The LLM summary of it.
    Smart,
}

impl SourceKind {
    /// The spelling stored in the `source_kind` column.
    pub const fn as_str(self) -> &'static str {
        match self {
            SourceKind::Raw => "raw",
            SourceKind::Smart => "smart",
        }
    }

    fn from_str(value: &str) -> Result<Self, StoreError> {
        match value {
            "raw" => Ok(SourceKind::Raw),
            "smart" => Ok(SourceKind::Smart),
            other => Err(StoreError::Corrupt(fs3_core::Error::InvalidConfig(
                format!("unknown embedding source kind {other:?}"),
            ))),
        }
    }
}

/// A vector on its way into the store.
///
/// Borrowed rather than owned because the caller has just received these from
/// an [`fs3_core::Embedder`] batch and has no reason to hand ownership over.
#[derive(Clone, Copy, Debug)]
pub struct NewEmbedding<'a> {
    /// `raw_hash` for [`SourceKind::Raw`], the summary's `text_hash` for
    /// [`SourceKind::Smart`].
    pub source_hash: &'a str,
    /// Which of the two this vector describes.
    pub source_kind: SourceKind,
    /// The vector itself, [`EMBEDDING_DIMENSIONS`] wide.
    pub vector: &'a [f32],
}

/// Store a batch of vectors under one embedding model, atomically.
///
/// `model_key` here names the EMBEDDING model, a different namespace from
/// `smart_content.model_key` (which names the summarising model). The two are
/// never compared.
///
/// # Errors
/// [`StoreError::Dimensions`] before touching the database when any vector is
/// the wrong width — the fix is a different model or a new `embeddings_<dim>`
/// table, so failing on the caller's terms beats a Postgres type error.
/// [`StoreError::Query`] when the transaction fails; nothing is written.
pub async fn put_embeddings(
    pool: &PgPool,
    model_key: &str,
    embeddings: &[NewEmbedding<'_>],
) -> Result<(), StoreError> {
    // Checked up front, so a bad batch costs no round trips and leaves no
    // partial write to reason about.
    if let Some(wrong) = embeddings
        .iter()
        .find(|row| row.vector.len() != EMBEDDING_DIMENSIONS)
    {
        return Err(StoreError::Dimensions {
            expected: EMBEDDING_DIMENSIONS,
            actual: wrong.vector.len(),
        });
    }

    let mut tx = pool.begin().await?;
    for row in embeddings {
        sqlx::query(
            "INSERT INTO embeddings_1024 (source_hash, source_kind, model_key, vector)
             VALUES ($1, $2, $3, $4)
             ON CONFLICT (source_hash, source_kind, model_key) DO UPDATE SET
               vector = EXCLUDED.vector",
        )
        .bind(row.source_hash)
        .bind(row.source_kind.as_str())
        .bind(model_key)
        .bind(Vector::from(row.vector.to_vec()))
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// One nearest-neighbour hit, resolved back to the element it is about.
#[derive(Clone, Debug, PartialEq)]
pub struct SimilarElement {
    /// The element, childless — a search result is a hit, not a subtree.
    pub element: Element,
    /// The blob the element was found in.
    pub blob_sha: String,
    /// The parser that produced it.
    pub parser_version: String,
    /// Whether the vector that matched was of the raw text or of its summary.
    pub source_kind: SourceKind,
    /// The summary, when the hit was a smart vector.
    pub smart: Option<fs3_core::Summary>,
    /// Cosine distance: 0.0 is identical, and nearest sorts first.
    pub distance: f64,
}

/// The `limit` nearest elements to `query`, nearest first.
///
/// The neighbour search is a CTE that finishes — `ORDER BY … LIMIT` — before
/// anything is joined, which is what lets the HNSW index answer it. Joining
/// first and sorting after would read every row.
///
/// Two resolutions happen after that, and both pick one representative on
/// purpose:
///
/// * a smart vector resolves through `smart_content.text_hash` to the raw hash
///   it describes. Identical summary text under two summarising models is one
///   `text_hash`, so the oldest is taken.
/// * a raw hash resolves to an element. The same body exists at many
///   `(blob, parser_version, address)` triples — that sharing is the point of
///   decision D2 — so the lowest element id is taken as the example. Resolving
///   a hit to every path that holds it is the ref layer's job, not this query's.
///
/// A vector whose source has no element row left (after a prune, say) is
/// dropped rather than returned as a hit with nothing behind it.
///
/// # Errors
/// [`StoreError::Dimensions`] when `query` is the wrong width;
/// [`StoreError::Query`] on failure; [`StoreError::Corrupt`] when a row carries
/// an unknown kind.
pub async fn query_embeddings(
    pool: &PgPool,
    model_key: &str,
    query: &[f32],
    limit: i64,
) -> Result<Vec<SimilarElement>, StoreError> {
    if query.len() != EMBEDDING_DIMENSIONS {
        return Err(StoreError::Dimensions {
            expected: EMBEDDING_DIMENSIONS,
            actual: query.len(),
        });
    }

    let rows = sqlx::query(
        "WITH nearest AS (
             SELECT source_hash, source_kind, vector <=> $1 AS distance
               FROM embeddings_1024
              WHERE model_key = $2
              ORDER BY vector <=> $1
              LIMIT $3
         )
         SELECT n.source_kind, n.distance,
                s.text AS smart_text, s.tags AS smart_tags,
                e.blob_sha, e.parser_version, e.kind, e.subkind, e.name,
                e.address, e.span_start, e.span_end, e.sibling_order, e.raw_text
           FROM nearest n
           LEFT JOIN LATERAL (
                SELECT sc.raw_hash, sc.text, sc.tags
                  FROM smart_content sc
                 WHERE n.source_kind = 'smart' AND sc.text_hash = n.source_hash
                 ORDER BY sc.created_at, sc.model_key
                 LIMIT 1
           ) s ON TRUE
           JOIN LATERAL (
                SELECT el.id, el.blob_sha, el.parser_version, el.kind, el.subkind,
                       el.name, el.address, el.span_start, el.span_end,
                       el.sibling_order, el.raw_text
                  FROM elements el
                 WHERE el.raw_hash = COALESCE(s.raw_hash, n.source_hash)
                 ORDER BY el.id
                 LIMIT 1
           ) e ON TRUE
          ORDER BY n.distance",
    )
    .bind(Vector::from(query.to_vec()))
    .bind(model_key)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    rows.iter().map(similar_from_row).collect()
}

fn similar_from_row(row: &sqlx::postgres::PgRow) -> Result<SimilarElement, StoreError> {
    let kind: String = row.try_get("kind")?;
    let element = Element::new(
        kind_from_str(&kind)?,
        row.try_get::<String, _>("subkind")?,
        row.try_get::<String, _>("name")?,
        row.try_get::<String, _>("address")?,
        fs3_core::Span::new(
            row.try_get::<i32, _>("span_start")? as u32,
            row.try_get::<i32, _>("span_end")? as u32,
        ),
        row.try_get::<String, _>("raw_text")?,
    )
    .with_sibling_order(row.try_get::<i32, _>("sibling_order")? as u32);

    let smart_text: Option<String> = row.try_get("smart_text")?;
    let smart = match smart_text {
        Some(text) => Some(fs3_core::Summary {
            text,
            tags: row.try_get("smart_tags")?,
        }),
        None => None,
    };

    Ok(SimilarElement {
        element,
        blob_sha: row.try_get("blob_sha")?,
        parser_version: row.try_get("parser_version")?,
        source_kind: SourceKind::from_str(&row.try_get::<String, _>("source_kind")?)?,
        smart,
        distance: row.try_get("distance")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_kind_round_trips_through_its_stored_spelling() {
        for kind in [SourceKind::Raw, SourceKind::Smart] {
            assert_eq!(SourceKind::from_str(kind.as_str()).unwrap(), kind);
        }
        assert!(SourceKind::from_str("summary").is_err());
    }
}
