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
use sqlx::types::Json;
use std::collections::HashSet;

use crate::elements::kind_from_str;
use crate::{PgPool, StoreError};

/// The vector width `embeddings_1024` holds.
pub const EMBEDDING_DIMENSIONS: usize = 1024;

/// Make the HNSW scan keep going until the caller's `LIMIT` is filled.
///
/// pgvector's default is one pass: the index yields `hnsw.ef_search`
/// candidates and stops, whatever the surrounding `WHERE` then does to them.
/// That is the correct default for an unfiltered nearest-neighbour query and
/// the wrong one for every query fs3 asks, because fs3 always filters — by
/// repository, by path, by content kind — and a filter that is selective
/// against the whole index silently eats the batch.
///
/// `strict_order` rather than `relaxed_order`: the surface promises the
/// `limit` NEAREST elements, relaxed order does not promise that the batch it
/// returns is the true top-k, and a wrong set sorted convincingly by the outer
/// `ORDER BY` is worse than a slower right one. On the index this was measured
/// against, strict order was also the faster of the two.
///
/// `SET LOCAL`, so it lives and dies with one transaction and cannot follow a
/// pooled connection to its next borrower.
///
/// Safe on a pgvector too old to know the setting: `hnsw.iterative_scan` is a
/// prefixed (custom) GUC, and Postgres accepts an assignment to an unclaimed
/// prefix as a placeholder rather than erroring — an older extension simply
/// ignores it instead of taking search down.
const ITERATIVE_SCAN: &str = "SET LOCAL hnsw.iterative_scan = strict_order";

/// Start with enough vectors for ordinary two- or three-chunk elements while
/// keeping the common ANN scan small.
const INITIAL_CANDIDATE_MULTIPLIER: i64 = 4;
/// Double only when the current candidate page is full and still under-fills
/// the requested number of distinct elements.
const CANDIDATE_GROWTH_FACTOR: i64 = 2;
/// Refuse rather than return a silently under-filled page after eight retries.
/// At the largest page a limit of ten examines at most 10,240 vectors.
const MAX_CANDIDATE_EXPANSIONS: usize = 8;

fn candidate_count(rows: &[sqlx::postgres::PgRow]) -> Result<i64, StoreError> {
    rows.first()
        .map(|row| row.try_get("candidate_count"))
        .transpose()
        .map(|count| count.unwrap_or(0))
        .map_err(StoreError::from)
}

fn candidate_limit_exhausted(limit: i64) -> StoreError {
    StoreError::Query(sqlx::Error::Protocol(format!(
        "semantic search could not fill {limit} distinct elements after \
         {MAX_CANDIDATE_EXPANSIONS} candidate expansions"
    )))
}

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
    /// Zero-based position of this vector's overlapping source-text chunk.
    pub chunk_no: i16,
    /// The vector itself, [`EMBEDDING_DIMENSIONS`] wide.
    pub vector: &'a [f32],
    /// Whether the text embedded was a PREFIX of its chunk because even that
    /// chunk exceeded the model's per-input cap.
    ///
    /// Retained as inventory for a deferred backfill of vectors written before
    /// chunking replaced whole-content prefix truncation.
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
/// # Why the key includes kind and chunk
///
/// The primary key is `(source_hash, source_kind, chunk_no, model_key)`.
/// Filtering on hash and model alone would treat a stored `raw` vector as
/// covering the `smart` vector for the same hash. The pre-check deliberately
/// answers at hash granularity: any chunk proves the atomic batch landed.
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

    // DISTINCT collapses a chunked source back to the hash-level pre-check.
    let owned: Vec<String> = hashes.iter().map(|hash| (*hash).to_string()).collect();
    let rows = sqlx::query(
        "SELECT DISTINCT source_hash FROM embeddings_1024
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
            "INSERT INTO embeddings_1024
               (source_hash, source_kind, chunk_no, model_key, vector, truncated)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (source_hash, source_kind, chunk_no, model_key) DO UPDATE SET
               vector = EXCLUDED.vector,
               truncated = EXCLUDED.truncated",
        )
        .bind(row.source_hash)
        .bind(row.source_kind.as_str())
        .bind(row.chunk_no)
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

    let mut candidate_limit = limit.saturating_mul(INITIAL_CANDIDATE_MULTIPLIER);
    for expansion in 0..=MAX_CANDIDATE_EXPANSIONS {
        let rows = sqlx::query(
            "WITH candidate_vectors AS MATERIALIZED (
                 SELECT source_hash, source_kind, chunk_no,
                        vector <=> $1 AS distance
                   FROM embeddings_1024
                  WHERE model_key = $2
                  ORDER BY vector <=> $1
                  LIMIT $3
             ),
             candidate_meta AS (
                 SELECT count(*)::bigint AS candidate_count
                   FROM candidate_vectors
             ),
             resolved AS (
                 SELECT c.source_hash, c.source_kind, c.chunk_no, c.distance,
                        s.text, s.tags, s.extras,
                        e.id AS element_id,
                        e.blob_sha, e.parser_version, e.kind, e.subkind, e.name,
                        e.address, e.span_start, e.span_end, e.sibling_order,
                        e.raw_text, e.ddoc
                   FROM candidate_vectors c
                   LEFT JOIN LATERAL (
                        SELECT sc.raw_hash, sc.text, sc.tags, sc.extras
                          FROM smart_content sc
                         WHERE c.source_kind = 'smart' AND sc.text_hash = c.source_hash
                         ORDER BY sc.created_at, sc.model_key
                         LIMIT 1
                   ) s ON TRUE
                   JOIN LATERAL (
                        SELECT el.id, el.blob_sha, el.parser_version, el.kind,
                               el.subkind, el.name, el.address, el.span_start,
                               el.span_end, el.sibling_order, el.raw_text, el.ddoc
                          FROM elements el
                         WHERE el.raw_hash = COALESCE(s.raw_hash, c.source_hash)
                         ORDER BY el.id
                         LIMIT 1
                   ) e ON TRUE
             ),
             nearest AS (
                 SELECT DISTINCT ON (element_id) *
                   FROM resolved
                  ORDER BY element_id, distance, source_kind, source_hash, chunk_no
             )
             SELECT n.source_kind, n.distance,
                    n.text, n.tags, n.extras,
                    n.blob_sha, n.parser_version, n.kind, n.subkind, n.name,
                    n.address, n.span_start, n.span_end, n.sibling_order, n.raw_text,
                    n.ddoc, m.candidate_count
               FROM nearest n
               CROSS JOIN candidate_meta m
              ORDER BY n.distance, n.element_id
              LIMIT $4",
        )
        .bind(Vector::from(query.to_vec()))
        .bind(model_key)
        .bind(candidate_limit)
        .bind(limit)
        .fetch_all(pool)
        .await?;

        let scanned = candidate_count(&rows)?;
        if rows.len() as i64 >= limit || scanned < candidate_limit {
            return rows.iter().map(similar_from_row).collect();
        }
        if expansion == MAX_CANDIDATE_EXPANSIONS {
            return Err(candidate_limit_exhausted(limit));
        }
        candidate_limit = candidate_limit.saturating_mul(CANDIDATE_GROWTH_FACTOR);
    }

    unreachable!("candidate expansion loop returns or refuses at its bound")
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
    /// Only turns belonging to this exact conversation guid.
    ///
    /// This predicate is repeated at every chooser, like the ownership and
    /// kind predicates: applying it only to admission can resolve a shared
    /// raw hash through a different transcript.
    pub conversation: Option<String>,
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
    /// Only ddoc rows carrying one of these raw minted-id prefixes.
    pub id_kinds: Option<Vec<String>>,
    /// `true` selects known non-terminal rows; `false` known terminal rows.
    /// Rows whose gate membership is unknown match neither value.
    pub gate_open: Option<bool>,
    /// Only ddoc rows declaring this schema, verbatim.
    pub ddoc_schema: Option<String>,
    /// How many hits to return.
    pub limit: i64,
}

/// Ownership scope for [`anchor_has_vectors`].
///
/// Deliberately cannot represent ranked, kind, source-state, or ddoc-content
/// predicates: this probe answers whether a scope has searchable content, not
/// whether a content filter matches it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AnchorScope<'a> {
    /// Repository identity to probe.
    pub repo: Option<&'a str>,
    /// Registered worktree root to probe.
    pub worktree: Option<&'a str>,
    /// Live repository-relative path pattern to probe.
    pub path: Option<&'a str>,
}

/// What an indexed-path filter can reach inside one ownership scope.
///
/// This is deliberately independent of embeddings and ranking: it distinguishes
/// a filter that matches no indexed path from a valid path whose content did not
/// rank for a query.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PathFilterProbe {
    /// Whether at least one indexed path matches the SQL `LIKE` pattern.
    pub matches: bool,
    /// Distinct first path segments available in the same scope, sorted.
    pub top_level_entries: Vec<String>,
    /// Live file-backed elements reachable through matching paths.
    pub matching_elements: i64,
}

impl Default for SearchFilters {
    fn default() -> Self {
        SearchFilters {
            repo: None,
            worktree: None,
            conversation: None,
            path: None,
            source: None,
            max_distance: None,
            kinds: None,
            id_kinds: None,
            gate_open: None,
            ddoc_schema: None,
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

#[cfg(test)]
const ADMISSION_JOIN_SQL: &str = r#"JOIN admitted_sources admitted
        ON admitted.source_hash = e.source_hash
       AND admitted.source_kind = e.source_kind"#;

const SEARCH_ELEMENTS_SQL: &str = r#"WITH admitted_elements AS MATERIALIZED (
    SELECT admitted.id, admitted.raw_hash
      FROM elements admitted
     WHERE ($8::text[] IS NULL OR admitted.kind = ANY($8))
       AND ($10::text[] IS NULL
            OR admitted.ddoc->>'id_kind' = ANY($10))
       AND ($11::boolean IS NULL
            OR (CASE
                  WHEN jsonb_typeof(admitted.ddoc->'derived_state') = 'object'
                  THEN (admitted.ddoc->'derived_state'->>'complete')::boolean
                  ELSE (admitted.ddoc->>'gate_terminal')::boolean
                END IS NOT NULL
                AND CASE
                  WHEN jsonb_typeof(admitted.ddoc->'derived_state') = 'object'
                  THEN (admitted.ddoc->'derived_state'->>'complete')::boolean
                  ELSE (admitted.ddoc->>'gate_terminal')::boolean
                END = NOT $11))
       AND ($12::text IS NULL
            OR admitted.ddoc->>'schema' = $12)
       AND ($13::text IS NULL
            OR strpos(admitted.address, 'conv:' || $13 || '#t') = 1)
       AND ($6::text IS NULL AND $7::text IS NULL AND $9::text IS NULL
            OR EXISTS (
                 SELECT 1
                   FROM worktree_files f
                   JOIN worktrees w ON w.id = f.worktree_id
                   JOIN repos r     ON r.id = w.repo_id
                  WHERE f.blob_sha = admitted.blob_sha
                    AND ($6::text IS NULL OR r.identity = $6)
                    AND ($7::text IS NULL OR f.path LIKE $7)
                    AND ($9::text IS NULL OR w.root_path = $9))
            OR EXISTS (
                 SELECT 1
                   FROM turns t
                   JOIN conversations c ON c.guid = t.conversation_id
                  WHERE t.blob_sha = admitted.blob_sha
                    AND ($6::text IS NULL OR c.repo_identity = $6)
                    AND ($7::text IS NULL OR c.worktree LIKE $7)
                    AND ($9::text IS NULL OR c.worktree IS NULL OR c.worktree = $9)))
),
smart_map AS MATERIALIZED (
    SELECT candidate.raw_hash, candidate.model_key, candidate.text,
           candidate.text_hash, candidate.tags, candidate.extras,
           candidate.created_at
      FROM smart_content candidate
      JOIN (SELECT DISTINCT raw_hash FROM admitted_elements) admitted
        ON admitted.raw_hash = candidate.raw_hash
),
admitted_sources AS MATERIALIZED (
    SELECT raw_hash AS source_hash, 'raw'::text AS source_kind
      FROM admitted_elements
    UNION
    SELECT text_hash AS source_hash, 'smart'::text AS source_kind
      FROM smart_map
),
candidate_vectors AS MATERIALIZED (
    SELECT e.source_hash, e.source_kind, e.chunk_no,
           e.vector <=> $1 AS distance
      FROM embeddings_1024 e
      JOIN admitted_sources admitted
        ON admitted.source_hash = e.source_hash
       AND admitted.source_kind = e.source_kind
     WHERE e.model_key = $2
       AND ($4::text IS NULL OR e.source_kind = $4)
       AND ($5::float8 IS NULL OR (e.vector <=> $1) <= $5)
     ORDER BY e.vector <=> $1
     LIMIT $14
),
candidate_meta AS (
    SELECT count(*)::bigint AS candidate_count
      FROM candidate_vectors
),
resolved AS (
    SELECT c.source_hash, c.source_kind, c.chunk_no, c.distance,
           s.text, s.tags, s.extras,
           e.id AS element_id,
           e.blob_sha, e.parser_version, e.kind, e.subkind, e.name,
           e.address, e.span_start, e.span_end, e.sibling_order,
           e.raw_text, e.ddoc
      FROM candidate_vectors c
      LEFT JOIN LATERAL (
           SELECT candidate.raw_hash, candidate.text,
                  candidate.tags, candidate.extras
             FROM smart_map candidate
            WHERE c.source_kind = 'smart'
              AND candidate.text_hash = c.source_hash
            ORDER BY candidate.created_at, candidate.model_key,
                     candidate.raw_hash
            LIMIT 1
      ) s ON TRUE
      JOIN LATERAL (
           SELECT el.id, el.blob_sha, el.parser_version,
                  el.kind, el.subkind, el.name, el.address,
                  el.span_start, el.span_end, el.sibling_order,
                  el.raw_text, el.ddoc
             FROM admitted_elements admitted
             JOIN elements el ON el.id = admitted.id
            WHERE admitted.raw_hash = COALESCE(s.raw_hash, c.source_hash)
            ORDER BY admitted.id
            LIMIT 1
      ) e ON TRUE
),
nearest AS (
    SELECT DISTINCT ON (element_id) *
      FROM resolved
     ORDER BY element_id, distance, source_kind, source_hash, chunk_no
)
SELECT n.source_kind, n.distance,
       n.text, n.tags, n.extras,
       n.blob_sha, n.parser_version, n.kind, n.subkind, n.name,
       n.address, n.span_start, n.span_end, n.sibling_order, n.raw_text,
       n.ddoc,
       COALESCE(live.identity, anchored.identity) AS identity,
       COALESCE(live.root_path, anchored.root_path) AS root_path,
       live.path, m.candidate_count
  FROM nearest n
  CROSS JOIN candidate_meta m
  LEFT JOIN LATERAL (
       SELECT r.identity, w.root_path, f.path
         FROM worktree_files f
         JOIN worktrees w ON w.id = f.worktree_id
         JOIN repos r     ON r.id = w.repo_id
        WHERE f.blob_sha = n.blob_sha
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
        WHERE t.blob_sha = n.blob_sha
          AND ($6::text IS NULL OR c.repo_identity = $6)
          AND ($7::text IS NULL OR c.worktree LIKE $7)
          AND ($9::text IS NULL OR c.worktree IS NULL OR c.worktree = $9)
          AND ($13::text IS NULL OR c.guid = $13::uuid)
        ORDER BY c.repo_identity, c.worktree
        LIMIT 1
  ) anchored ON TRUE
 ORDER BY n.distance, n.element_id
 LIMIT $3"#;

/// The `limit` nearest elements to `query`, narrowed by `filters`, nearest first.
///
/// The filtered sibling of [`query_embeddings`]. [`SEARCH_ELEMENTS_SQL`] resolves
/// eligible elements and smart mappings once, reduces them to distinct source
/// keys, and keeps that key set on the inner side of the ordered HNSW scan. The
/// plan-shape test guards both halves: hash admission must not become correlated,
/// and the vector index must remain the source of nearest candidates.
///
/// The `<=>` operator is not interchangeable here: `embeddings_1024`'s index is
/// built for `vector_cosine_ops`, and a query written with `<->` gets a
/// sequential scan with no error to notice.
///
/// # Why this runs in a transaction
///
/// Keeping the filters inside the CTE buys the index scan, and it costs
/// something that has to be paid for explicitly: an HNSW scan yields at most
/// `hnsw.ef_search` candidates, and every predicate above is applied to THAT
/// handful rather than to the index. A selective anchor — one small repository
/// inside an index holding several — can therefore delete every candidate and
/// leave the CTE empty while thousands of matching vectors sit one hop
/// further out. Nothing surfaces: no error, no warning, just an answer that is
/// short or absent, and `--min-score` cannot be blamed because the floor never
/// gets a row to reject. Measured on a four-repository index where the
/// searched repository held 9.5% of the vectors, twelve ordinary questions
/// asked for ten hits each and were answered with 19 of 120.
///
/// [`ITERATIVE_SCAN`] is the remedy pgvector 0.8 added for exactly this: keep
/// pulling batches until the `LIMIT` is satisfied or the scan budget runs out.
/// The same twelve questions then return 120 of 120.
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

    let mut tx = pool.begin().await?;
    sqlx::query(ITERATIVE_SCAN).execute(&mut *tx).await?;

    // Every filter is bound unconditionally with a NULL-means-any guard, so
    // there is ONE statement text whatever the caller asked for. A query built
    // by string concatenation would have a different plan per flag combination
    // and could not be read as a single thing.
    // Bind map: $1 vector, $2 model, $3 element limit, $4 source, $5 distance,
    // $6 repo, $7 path, $8 kinds, $9 worktree, $10 id_kinds,
    // $11 gate_open, $12 ddoc_schema, $13 conversation, $14 vector candidate
    // limit. Keep SQL and binds in this order: these types overlap, so a shifted
    // parameter can compile and answer incorrectly.
    let mut candidate_limit = filters.limit.saturating_mul(INITIAL_CANDIDATE_MULTIPLIER);
    for expansion in 0..=MAX_CANDIDATE_EXPANSIONS {
        let rows = sqlx::query(SEARCH_ELEMENTS_SQL)
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
            .bind(filters.id_kinds.as_deref())
            .bind(filters.gate_open)
            .bind(filters.ddoc_schema.as_deref())
            .bind(filters.conversation.as_deref())
            .bind(candidate_limit)
            .fetch_all(&mut *tx)
            .await?;

        let scanned = candidate_count(&rows)?;
        if rows.len() as i64 >= filters.limit || scanned < candidate_limit {
            tx.commit().await?;
            return rows
                .iter()
                .map(|row| {
                    Ok(SearchHit {
                        similar: similar_from_row(row)?,
                        identity: row.try_get("identity")?,
                        root_path: row.try_get("root_path")?,
                        path: row.try_get("path")?,
                    })
                })
                .collect();
        }
        if expansion == MAX_CANDIDATE_EXPANSIONS {
            return Err(candidate_limit_exhausted(filters.limit));
        }
        candidate_limit = candidate_limit.saturating_mul(CANDIDATE_GROWTH_FACTOR);
    }

    unreachable!("candidate expansion loop returns or refuses at its bound")
}

/// Does `model_key` hold reachable content in `scope`?
///
/// This answers an OWNERSHIP question, not whether content predicates matched.
/// [`AnchorScope`] makes every content filter unrepresentable so a legitimate
/// empty result cannot become a false "repository is not indexed" diagnosis
/// when a new predicate is added.
///
/// Raw vectors whose element is reachable through either a live file or a
/// conversation anchor. The ownership legs mirror search admission; otherwise
/// mixed default search could diagnose a conversation-only repository as empty.
///
/// # Errors
/// [`StoreError::Query`] on failure.
pub async fn anchor_has_vectors(
    pool: &PgPool,
    model_key: &str,
    scope: &AnchorScope<'_>,
) -> Result<bool, StoreError> {
    // Bind map: $1 model, $2 repo, $3 path, $4 worktree. Content predicates have no slot.
    let found: bool = sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1
               FROM embeddings_1024 e
               JOIN elements el ON el.raw_hash = e.source_hash
              WHERE e.model_key = $1
                AND e.source_kind = 'raw'
                AND (EXISTS (
                     SELECT 1
                       FROM worktree_files f
                       JOIN worktrees w ON w.id = f.worktree_id
                       JOIN repos r ON r.id = w.repo_id
                      WHERE f.blob_sha = el.blob_sha
                        AND ($2::text IS NULL OR r.identity = $2)
                        AND ($3::text IS NULL OR f.path LIKE $3)
                        AND ($4::text IS NULL OR w.root_path = $4))
                     OR EXISTS (
                     SELECT 1
                       FROM turns t
                       JOIN conversations c ON c.guid = t.conversation_id
                      WHERE t.blob_sha = el.blob_sha
                        AND ($2::text IS NULL OR c.repo_identity = $2)
                        AND ($3::text IS NULL OR c.worktree LIKE $3)
                        AND ($4::text IS NULL OR c.worktree IS NULL OR c.worktree = $4)))
                )",
    )
    .bind(model_key)
    .bind(scope.repo)
    .bind(scope.path)
    .bind(scope.worktree)
    .fetch_one(pool)
    .await?;

    Ok(found)
}

/// Check whether a path pattern matches anything and summarize the scoped layout.
///
/// # Errors
/// [`StoreError::Query`] on failure.
pub async fn path_filter_probe(
    pool: &PgPool,
    repo: Option<&str>,
    worktree: Option<&str>,
    path: &str,
    kinds: Option<&[ElementKind]>,
) -> Result<PathFilterProbe, StoreError> {
    let row = sqlx::query(
        "SELECT COALESCE(bool_or(f.path LIKE $3), false) AS matches,
                COALESCE(
                    array_agg(DISTINCT split_part(f.path, '/', 1)
                              ORDER BY split_part(f.path, '/', 1))
                        FILTER (WHERE f.path <> ''),
                    ARRAY[]::text[]
                ) AS top_level_entries,
                count(el.id) FILTER (
                    WHERE f.path LIKE $3
                      AND ($4::text[] IS NULL OR el.kind = ANY($4))
                )::bigint AS matching_elements
           FROM worktree_files f
           JOIN worktrees w ON w.id = f.worktree_id
           JOIN repos r     ON r.id = w.repo_id
           LEFT JOIN elements el ON el.blob_sha = f.blob_sha
          WHERE ($1::text IS NULL OR r.identity = $1)
            AND ($2::text IS NULL OR w.root_path = $2)",
    )
    .bind(repo)
    .bind(worktree)
    .bind(path)
    .bind(kinds.map(|kinds| {
        kinds
            .iter()
            .map(|kind| (*kind).as_str())
            .collect::<Vec<_>>()
    }))
    .fetch_one(pool)
    .await?;

    Ok(PathFilterProbe {
        matches: row.try_get("matches")?,
        top_level_entries: row.try_get("top_level_entries")?,
        matching_elements: row.try_get("matching_elements")?,
    })
}
fn similar_from_row(row: &sqlx::postgres::PgRow) -> Result<SimilarElement, StoreError> {
    let kind: String = row.try_get("kind")?;
    let mut element = Element::new(
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
    element.ddoc = row
        .try_get::<Option<Json<fs3_core::DdocMeta>>, _>("ddoc")?
        .map(|Json(meta)| Box::new(meta));

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

    const SHAPE_MODEL: &str = "search-plan-shape@1024";

    async fn shape_database() -> (String, PgPool, PgPool) {
        use std::time::{SystemTime, UNIX_EPOCH};

        let base_url = fs3_testkit::test_database_url();
        let (maintenance_url, _) = crate::maintenance_url(&base_url).unwrap();
        let admin = crate::connect(&maintenance_url).await.unwrap();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let name = format!("fs3_search_plan_{}_{}", std::process::id(), nanos);
        crate::create_database(&admin, &name).await.unwrap();
        let url = crate::database_url(&base_url, &name).unwrap();
        let pool = crate::connect(&url).await.unwrap();
        crate::migrate(&pool).await.unwrap();
        (name, pool, admin)
    }

    async fn seed_search_plan_corpus(pool: &PgPool) {
        sqlx::query(
            "INSERT INTO elements
                 (blob_sha, parser_version, kind, subkind, name, address,
                  span_start, span_end, sibling_order, raw_text, raw_hash, enrich)
             SELECT 'blob-' || n, 'search-plan@1', 'function', 'function_item',
                    'shape_' || n, 'src/shape.rs::shape_' || n,
                    1, 1, n, 'shape body ' || n, 'raw-' || n, false
               FROM generate_series(1, 50000) AS n",
        )
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO smart_content
                 (raw_hash, model_key, text, text_hash, tags)
             SELECT 'raw-' || n, 'search-plan-summary@1',
                    'shape summary ' || n, 'smart-' || n, ARRAY['shape']::text[]
               FROM generate_series(1, 10000) AS n",
        )
        .execute(pool)
        .await
        .unwrap();

        let query = shape_vector();
        sqlx::query(
            "INSERT INTO embeddings_1024
                 (source_hash, source_kind, chunk_no, model_key, vector, truncated)
             SELECT text_hash, 'smart', 0, $1, $2, false
               FROM smart_content",
        )
        .bind(SHAPE_MODEL)
        .bind(Vector::from(query))
        .execute(pool)
        .await
        .unwrap();
        sqlx::query("ANALYZE elements, smart_content, embeddings_1024")
            .execute(pool)
            .await
            .unwrap();
    }

    fn shape_vector() -> Vec<f32> {
        let mut vector = vec![0.0; EMBEDDING_DIMENSIONS];
        vector[0] = 1.0;
        vector
    }

    fn old_search_elements_sql() -> String {
        let without_join = SEARCH_ELEMENTS_SQL.replacen(ADMISSION_JOIN_SQL, "", 1);
        assert_ne!(
            without_join, SEARCH_ELEMENTS_SQL,
            "admission join marker drifted"
        );
        let distance_filter = "       AND ($5::float8 IS NULL OR (e.vector <=> $1) <= $5)";
        let old_admission = include_str!("../tests/fixtures/search_admission_old.sql")
            .lines()
            .map(|line| format!("       {line}"))
            .collect::<Vec<_>>()
            .join("\n");
        let old = without_join.replacen(
            distance_filter,
            &format!("{distance_filter}\n{old_admission}"),
            1,
        );
        assert_ne!(old, without_join, "distance filter marker drifted");
        old
    }

    async fn explain_search(pool: &PgPool, sql: &str) -> serde_json::Value {
        let statement = format!("EXPLAIN (ANALYZE, BUFFERS, VERBOSE, FORMAT JSON) {sql}");
        let row = sqlx::query(&statement)
            .bind(Vector::from(shape_vector()))
            .bind(SHAPE_MODEL)
            .bind(40_i64)
            .bind(Option::<&str>::None)
            .bind(Option::<f64>::None)
            .bind(Option::<&str>::None)
            .bind(Option::<&str>::None)
            .bind(Option::<Vec<String>>::None)
            .bind(Option::<&str>::None)
            .bind(Option::<Vec<String>>::None)
            .bind(Option::<bool>::None)
            .bind(Option::<&str>::None)
            .bind(Option::<&str>::None)
            .bind(160_i64)
            .fetch_one(pool)
            .await
            .unwrap();
        row.try_get::<Json<serde_json::Value>, _>(0).unwrap().0
    }

    fn visit_plan(
        value: &serde_json::Value,
        visit: &mut impl FnMut(&serde_json::Map<String, serde_json::Value>),
    ) {
        match value {
            serde_json::Value::Object(object) => {
                if object.contains_key("Node Type") {
                    visit(object);
                }
                for child in object.values() {
                    visit_plan(child, visit);
                }
            }
            serde_json::Value::Array(values) => {
                for child in values {
                    visit_plan(child, visit);
                }
            }
            _ => {}
        }
    }

    fn contains_relation(value: &serde_json::Value, relation: &str) -> bool {
        match value {
            serde_json::Value::Object(object) => {
                object
                    .get("Relation Name")
                    .and_then(serde_json::Value::as_str)
                    == Some(relation)
                    || object
                        .values()
                        .any(|child| contains_relation(child, relation))
            }
            serde_json::Value::Array(values) => values
                .iter()
                .any(|child| contains_relation(child, relation)),
            _ => false,
        }
    }

    fn materializes_elements(value: &serde_json::Value) -> bool {
        match value {
            serde_json::Value::Object(object) => {
                (object.get("Node Type").and_then(serde_json::Value::as_str) == Some("Materialize")
                    && object
                        .get("Plans")
                        .is_some_and(|plans| contains_relation(plans, "elements")))
                    || object.values().any(materializes_elements)
            }
            serde_json::Value::Array(values) => values.iter().any(materializes_elements),
            _ => false,
        }
    }

    fn smart_content_max_loops(plan: &serde_json::Value) -> Option<f64> {
        let mut loops: Option<f64> = None;
        visit_plan(plan, &mut |node| {
            if node
                .get("Relation Name")
                .and_then(serde_json::Value::as_str)
                == Some("smart_content")
            {
                if let Some(actual) = node.get("Actual Loops").and_then(serde_json::Value::as_f64) {
                    loops = Some(loops.map_or(actual, |current| current.max(actual)));
                }
            }
        });
        loops
    }

    fn candidate_vector_target_count(plan: &serde_json::Value) -> Option<usize> {
        let mut count = None;
        visit_plan(plan, &mut |node| {
            if node.get("Subplan Name").and_then(serde_json::Value::as_str)
                == Some("CTE candidate_vectors")
            {
                count = node
                    .get("Output")
                    .and_then(serde_json::Value::as_array)
                    .map(Vec::len);
            }
        });
        count
    }

    fn uses_vector_index(plan: &serde_json::Value) -> bool {
        let mut found = false;
        visit_plan(plan, &mut |node| {
            found |= node.get("Index Name").and_then(serde_json::Value::as_str)
                == Some("embeddings_1024_vector_idx");
        });
        found
    }

    #[tokio::test]
    async fn search_plan_shape_bounds_smart_content_work_and_rejects_the_old_query() {
        let (name, pool, admin) = shape_database().await;
        seed_search_plan_corpus(&pool).await;

        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM elements")
                .fetch_one(&pool)
                .await
                .unwrap(),
            50_000
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM smart_content")
                .fetch_one(&pool)
                .await
                .unwrap(),
            10_000
        );

        let shipped = explain_search(&pool, SEARCH_ELEMENTS_SQL).await;
        let old = explain_search(&pool, &old_search_elements_sql()).await;

        pool.close().await;
        crate::drop_database(&admin, &name).await.unwrap();
        admin.close().await;

        let shipped_loops = smart_content_max_loops(&shipped)
            .unwrap_or_else(|| panic!("shipped plan has no smart_content node: {shipped:#}"));
        assert!(
            shipped_loops <= 160.0,
            "smart_content loops {shipped_loops} exceed candidate_limit 160: {shipped:#}"
        );
        assert!(
            !materializes_elements(&shipped),
            "shipped plan materializes an elements scan: {shipped:#}"
        );
        assert_eq!(
            candidate_vector_target_count(&shipped),
            Some(4),
            "candidate_vectors must carry source_hash, source_kind, chunk_no, distance only: {shipped:#}"
        );
        assert!(
            shipped[0].get("JIT").is_none(),
            "JIT must not trigger for the shipped query: {shipped:#}"
        );
        assert!(
            uses_vector_index(&shipped),
            "the HNSW index must remain the ordered candidate source: {shipped:#}"
        );

        let old_loops = smart_content_max_loops(&old).unwrap_or(f64::INFINITY);
        assert!(
            old_loops > 160.0 || materializes_elements(&old),
            "mutation failed: old admission unexpectedly satisfies the shipped shape: {old:#}"
        );
    }
}
