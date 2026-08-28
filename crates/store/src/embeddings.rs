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

use fs3_core::{Element, ElementKind};
use pgvector::Vector;
use sqlx::Row;
use std::collections::HashSet;

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

    pub(crate) fn from_str(value: &str) -> Result<Self, StoreError> {
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
    /// Whether the text embedded was a PREFIX of the content `source_hash`
    /// names, because the whole of it exceeded the model's per-input cap.
    ///
    /// The row is still keyed by the original hash — a truncated embedding is
    /// THE embedding for that content — so this flag is the only thing that
    /// distinguishes complete coverage from partial. See migration 0010.
    pub truncated: bool,
}

/// Which of these hashes already have a vector, for this model and this kind.
///
/// The cost check before an embed call: a re-emitted job whose vectors are all
/// stored should cost nothing, not a full re-embed. Content-addressed work is
/// re-emitted deliberately (a crash between parse and enrichment must not
/// strand elements), and this is what makes RE-EXECUTION free rather than just
/// re-execution CORRECT.
///
/// # Why all three key columns
///
/// The primary key is `(source_hash, source_kind, model_key)`. Filtering on
/// hash and model alone would treat a stored `raw` vector as covering the
/// `smart` vector for the same hash — the two are different text and different
/// meaning. That would silently under-embed and leave a permanently incomplete
/// index that looks exactly like a working one, which is the same failure
/// class as a misaligned batch and just as undetectable after the fact.
///
/// # Errors
/// [`StoreError::Query`] when the lookup fails.
pub async fn existing_embedding_hashes(
    pool: &PgPool,
    model_key: &str,
    source_kind: SourceKind,
    hashes: &[&str],
) -> Result<HashSet<String>, StoreError> {
    if hashes.is_empty() {
        return Ok(HashSet::new());
    }

    // ANY($3) over the primary key: one round trip whatever the batch size,
    // and an index lookup rather than a scan of the settled history.
    let owned: Vec<String> = hashes.iter().map(|hash| (*hash).to_string()).collect();
    let rows = sqlx::query(
        "SELECT source_hash FROM embeddings_1024
          WHERE model_key = $1 AND source_kind = $2 AND source_hash = ANY($3)",
    )
    .bind(model_key)
    .bind(source_kind.as_str())
    .bind(&owned)
    .fetch_all(pool)
    .await?;

    rows.iter()
        .map(|row| Ok(row.try_get("source_hash")?))
        .collect()
}

/// Every embedding model that has vectors, and how many.
///
/// The question behind an empty search: is there NOTHING indexed, or is there
/// an index the active model cannot see? Vectors are keyed by `model_key`, so
/// changing embedder — or changing width — makes an entire existing index
/// invisible without deleting a row of it. Reported as "no results", that is
/// indistinguishable from "your code does not contain that", which is the
/// worst possible answer because it is a confident lie.
///
/// Ordered by count, so the biggest index is named first.
///
/// # Errors
/// [`StoreError::Query`] when the lookup fails.
pub async fn embedding_models(pool: &PgPool) -> Result<Vec<(String, i64)>, StoreError> {
    let rows = sqlx::query(
        "SELECT model_key, count(*) AS vectors
           FROM embeddings_1024
          GROUP BY model_key
          ORDER BY vectors DESC, model_key",
    )
    .fetch_all(pool)
    .await?;

    rows.iter()
        .map(|row| Ok((row.try_get("model_key")?, row.try_get("vectors")?)))
        .collect()
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
            "INSERT INTO embeddings_1024 (source_hash, source_kind, model_key, vector, truncated)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (source_hash, source_kind, model_key) DO UPDATE SET
               vector = EXCLUDED.vector,
               truncated = EXCLUDED.truncated",
        )
        .bind(row.source_hash)
        .bind(row.source_kind.as_str())
        .bind(model_key)
        .bind(Vector::from(row.vector.to_vec()))
        .bind(row.truncated)
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
                s.text, s.tags, s.extras,
                e.blob_sha, e.parser_version, e.kind, e.subkind, e.name,
                e.address, e.span_start, e.span_end, e.sibling_order, e.raw_text
           FROM nearest n
           LEFT JOIN LATERAL (
                SELECT sc.raw_hash, sc.text, sc.tags, sc.extras
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

/// Which candidates a search is allowed to rank, and how many to return.
///
/// Every field here becomes a predicate INSIDE the neighbour CTE (workshop 003:
/// "filters narrow candidates in SQL — never post-hoc in app code"). Filtering
/// after the `LIMIT` would silently return fewer rows than asked for, and
/// filtering after a full fetch would read every vector in the table.
#[derive(Clone, Debug, PartialEq)]
pub struct SearchFilters {
    /// Only content held by a live path in this repository identity.
    pub repo: Option<String>,
    /// Only content held by this registered worktree root.
    pub worktree: Option<String>,
    /// Only content held by a live path matching this SQL `LIKE` pattern.
    pub path: Option<String>,
    /// Which element kinds may answer — the CONTENT-TYPE axis.
    ///
    /// Orthogonal to [`SearchFilters::source`], and the distinction is the one
    /// workshop 003's open question 1 was really about. `source` is the
    /// VECTOR-SPACE axis: raw text or summary, a column on `embeddings_1024`
    /// with a check constraint, and conversations are not a third value on it —
    /// a turn has a raw vector and a smart vector exactly like a function does.
    /// What makes a turn a turn is its element KIND, which is this.
    ///
    /// `None` places no restriction. A caller that wants code says so by
    /// naming the code kinds; there is no implicit default here, because "which
    /// content types answer by default" is a surface policy and the store is
    /// not the place to keep it.
    pub kinds: Option<Vec<ElementKind>>,
    /// Which vector space to search: raw text, summaries, or both.
    pub source: Option<SourceKind>,
    /// Cosine DISTANCE ceiling — a hit further than this is not returned.
    /// Expressed as the caller's minimum score: `1.0 - min_score`.
    pub max_distance: Option<f64>,
    /// How many hits to return.
    pub limit: i64,
}

impl Default for SearchFilters {
    fn default() -> Self {
        SearchFilters {
            repo: None,
            worktree: None,
            path: None,
            source: None,
            max_distance: None,
            kinds: None,
            limit: 10,
        }
    }
}

/// One nearest-neighbour hit, resolved to an element AND to where it lives.
#[derive(Clone, Debug, PartialEq)]
pub struct SearchHit {
    /// The content-layer answer: element, blob, parser, which space matched.
    pub similar: SimilarElement,
    /// The repository identity a live path holding this blob belongs to.
    ///
    /// `None` when no registered worktree holds the blob any more — content
    /// that outlived the checkout it came from, which decision D7 keeps on
    /// purpose. The hit is still real; only its address is stale.
    pub identity: Option<String>,
    /// The registered worktree root that supplied this hit.
    pub root_path: Option<String>,
    /// A live path holding the blob, relative to its worktree root.
    pub path: Option<String>,
}

/// The `limit` nearest elements to `query`, narrowed by `filters`, nearest first.
///
/// The filtered sibling of [`query_embeddings`], and the shape is the whole
/// point: the ref-layer join lives INSIDE the CTE as an `EXISTS` predicate, so
/// Postgres can still answer `ORDER BY vector <=> $1 LIMIT n` from the HNSW
/// index while excluding vectors no live path holds. Joining first and sorting
/// after — the obvious way to write it — reads every row in the table and turns
/// a millisecond query into a table scan.
///
/// The `<=>` operator is not interchangeable here: `embeddings_1024`'s index is
/// built for `vector_cosine_ops`, and a query written with `<->` gets a
/// sequential scan with no error to notice.
///
/// # Errors
/// [`StoreError::Dimensions`] when `query` is the wrong width;
/// [`StoreError::Query`] on failure; [`StoreError::Corrupt`] when a row carries
/// an unknown kind.
pub async fn search_elements(
    pool: &PgPool,
    model_key: &str,
    query: &[f32],
    filters: &SearchFilters,
) -> Result<Vec<SearchHit>, StoreError> {
    if query.len() != EMBEDDING_DIMENSIONS {
        return Err(StoreError::Dimensions {
            expected: EMBEDDING_DIMENSIONS,
            actual: query.len(),
        });
    }

    // Every filter is bound unconditionally with a NULL-means-any guard, so
    // there is ONE statement text whatever the caller asked for. A query built
    // by string concatenation would have a different plan per flag combination
    // and could not be read as a single thing.
    let rows = sqlx::query(
        "WITH nearest AS (
             SELECT source_hash, source_kind, vector <=> $1 AS distance
               FROM embeddings_1024 e
              WHERE model_key = $2
                AND ($4::text IS NULL OR source_kind = $4)
                AND ($5::float8 IS NULL OR (vector <=> $1) <= $5)
                -- The CONTENT-TYPE gate, and it is unconditional: a caller that
                -- names kinds must not be answered with another kind, whether
                -- or not it also named a repository.
                AND ($8::text[] IS NULL
                     OR EXISTS (
                          SELECT 1
                            FROM elements el
                           WHERE el.raw_hash = COALESCE(
                                   (SELECT sc.raw_hash FROM smart_content sc
                                     WHERE e.source_kind = 'smart'
                                       AND sc.text_hash = e.source_hash
                                     LIMIT 1),
                                   e.source_hash)
                             AND el.kind = ANY($8)))
                -- The ANCHOR gate, conditional so that content which outlived
                -- its checkout is still findable when nobody asked to narrow
                -- (decision D7). Two legs, because content reaches a repository
                -- two ways: code through the live path holding its blob, and a
                -- turn through the conversation ANCHORED to that repository.
                -- The caller worktree belongs here, before LIMIT: filtering a
                -- ranked page afterwards both under-fills it and can leak a
                -- foreign version when the caller's version lies beyond the cap.
                -- Without the second leg `--repo` would answer every
                -- conversation query with nothing, silently, while workshop 005
                -- promises the anchor filters compose.
                AND ($6::text IS NULL AND $7::text IS NULL AND $9::text IS NULL
                     OR EXISTS (
                          SELECT 1
                            FROM elements el
                           WHERE el.raw_hash = COALESCE(
                                   (SELECT sc.raw_hash FROM smart_content sc
                                     WHERE e.source_kind = 'smart'
                                       AND sc.text_hash = e.source_hash
                                     LIMIT 1),
                                   e.source_hash)
                             AND (EXISTS (
                                    SELECT 1
                                      FROM worktree_files f
                                      JOIN worktrees w ON w.id = f.worktree_id
                                      JOIN repos r     ON r.id = w.repo_id
                                     WHERE f.blob_sha = el.blob_sha
                                       AND ($6::text IS NULL OR r.identity = $6)
                                       AND ($7::text IS NULL OR f.path LIKE $7)
                                       AND ($9::text IS NULL OR w.root_path = $9))
                                  OR EXISTS (
                                    SELECT 1
                                      FROM turns t
                                      JOIN conversations c ON c.guid = t.conversation_id
                                     WHERE t.blob_sha = el.blob_sha
                                       AND ($6::text IS NULL OR c.repo_identity = $6)
                                       AND ($7::text IS NULL OR c.worktree LIKE $7)
                                       AND ($9::text IS NULL OR c.worktree = $9)))))
              ORDER BY vector <=> $1
              LIMIT $3
         )
         SELECT n.source_kind, n.distance,
                s.text, s.tags, s.extras,
                e.blob_sha, e.parser_version, e.kind, e.subkind, e.name,
                e.address, e.span_start, e.span_end, e.sibling_order, e.raw_text,
                COALESCE(live.identity, anchored.identity) AS identity,
                COALESCE(live.root_path, anchored.root_path) AS root_path,
                live.path
           FROM nearest n
           LEFT JOIN LATERAL (
                SELECT sc.raw_hash, sc.text, sc.tags, sc.extras
                  FROM smart_content sc
                 WHERE n.source_kind = 'smart' AND sc.text_hash = n.source_hash
                 ORDER BY sc.created_at, sc.model_key
                 LIMIT 1
           ) s ON TRUE
           JOIN LATERAL (
                SELECT el.id, el.blob_sha, el.parser_version, el.kind, el.subkind,
                       el.name, el.address, el.span_start, el.span_end,
                       el.sibling_order, el.raw_text, el.raw_hash
                  FROM elements el
                 WHERE el.raw_hash = COALESCE(s.raw_hash, n.source_hash)
                   -- Critic finding 4, and it is not belt-and-braces: this
                   -- resolver takes the LOWEST-id element carrying the hash, so
                   -- a turn that shares its text with code — which is exactly
                   -- what the dedupe makes common — would resolve to the code
                   -- element and be rendered with an `el:` address. The kind
                   -- has to be pinned on BOTH sides of the query.
                   AND ($8::text[] IS NULL OR el.kind = ANY($8))
                 ORDER BY el.id
                 LIMIT 1
           ) e ON TRUE
           LEFT JOIN LATERAL (
                SELECT r.identity, w.root_path, f.path
                  FROM worktree_files f
                  JOIN worktrees w ON w.id = f.worktree_id
                  JOIN repos r     ON r.id = w.repo_id
                 WHERE f.blob_sha = e.blob_sha
                   AND ($6::text IS NULL OR r.identity = $6)
                   AND ($7::text IS NULL OR f.path LIKE $7)
                   AND ($9::text IS NULL OR w.root_path = $9)
                 ORDER BY r.identity, w.root_path, f.path
                 LIMIT 1
           ) live ON TRUE
           LEFT JOIN LATERAL (
                SELECT c.repo_identity AS identity, c.worktree AS root_path
                  FROM turns t
                  JOIN conversations c ON c.guid = t.conversation_id
                 WHERE t.blob_sha = e.blob_sha
                   AND ($6::text IS NULL OR c.repo_identity = $6)
                   AND ($7::text IS NULL OR c.worktree LIKE $7)
                   AND ($9::text IS NULL OR c.worktree = $9)
                 ORDER BY c.repo_identity, c.worktree
                 LIMIT 1
           ) anchored ON TRUE
          ORDER BY n.distance",
    )
    .bind(Vector::from(query.to_vec()))
    .bind(model_key)
    .bind(filters.limit)
    .bind(filters.source.map(SourceKind::as_str))
    .bind(filters.max_distance)
    .bind(filters.repo.as_deref())
    .bind(filters.path.as_deref())
    .bind(
        filters
            .kinds
            .as_ref()
            .map(|kinds| kinds.iter().map(|kind| kind.as_str()).collect::<Vec<_>>()),
    )
    .bind(filters.worktree.as_deref())
    .fetch_all(pool)
    .await?;

    rows.iter()
        .map(|row| {
            Ok(SearchHit {
                similar: similar_from_row(row)?,
                identity: row.try_get("identity")?,
                root_path: row.try_get("root_path")?,
                path: row.try_get("path")?,
            })
        })
        .collect()
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

    // Decoded by the same function `get_smart_content` uses. Two decoders is
    // how `extras` came to be dropped on one path and kept on the other, and
    // the next field would have gone the same way.
    //
    // Borrowed rather than owned: this only asks whether the LEFT JOIN matched,
    // and allocating the text to answer that would throw it away immediately.
    let smart = match row.try_get::<Option<&str>, _>("text")? {
        Some(_) => Some(crate::smart::summary_from_row(row)?),
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
