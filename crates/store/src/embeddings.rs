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

/// The admission plan's conservative cost estimate used to trigger roughly
/// 281 ms of JIT compilation for a query whose HNSW work took 12 ms. This
/// statement is latency-sensitive and bounded by [`candidate_limit`], so JIT's
/// startup cost cannot amortize before the query returns.
const DISABLE_SEARCH_JIT: &str = "SET LOCAL jit = off";

/// Plan the search for the values it is given, every time.
///
/// sqlx prepares this statement once per pooled connection. After five runs
/// Postgres may switch to a generic plan, which cannot see the scope and on
/// prod (2026-10-15) replaced the HNSW arm with a sequential scan of every
/// vector: an accidental brute-force rank on a scoped search, and a sort of
/// 1.65M distances on an unscoped one. So the plan, the recall and the
/// latency depended on how old the connection was. Planning it fresh costs
/// 1 to 4 ms.
///
/// The keyword leg takes the same setting ([`plan_each_search`]): a generic
/// plan of a BM25 statement cannot route its operators through the ParadeDB
/// scan, seq-scans `elements`, and fails on `pdb.score` (prod, read-only,
/// 2026-10-15).
const CUSTOM_SEARCH_PLAN: &str = "SET LOCAL plan_cache_mode = force_custom_plan";

async fn configure_search_transaction(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
) -> Result<(), StoreError> {
    sqlx::query(ITERATIVE_SCAN).execute(&mut **tx).await?;
    sqlx::query(DISABLE_SEARCH_JIT).execute(&mut **tx).await?;
    plan_each_search(tx).await
}

/// Plan every statement of this search transaction for the values it is
/// given ([`CUSTOM_SEARCH_PLAN`]).
pub(crate) async fn plan_each_search(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
) -> Result<(), StoreError> {
    sqlx::query(CUSTOM_SEARCH_PLAN).execute(&mut **tx).await?;
    Ok(())
}

/// Bound every statement of this transaction to `budget`, when there is one.
pub(crate) async fn apply_budget(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    budget: Option<std::time::Duration>,
) -> Result<(), StoreError> {
    if let Some(budget) = budget {
        // Never under a second, so a budget that is nearly spent still lets
        // the pass it starts finish; the expansion loop is what stops early.
        // `SET` takes no bind parameters; the value is a whole number we made.
        let ms = budget.as_millis().clamp(1_000, i32::MAX as u128);
        sqlx::query(&format!("SET LOCAL statement_timeout = {ms}"))
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

/// The largest scope, in distinct source texts, that search compares in full
/// instead of through the HNSW index.
///
/// An exact scan of a scope costs one distance per vector in it, and it is
/// complete. The iterative ANN scan stops after `hnsw.max_scan_tuples` (20,000
/// by default) whatever it has found, so a scope that is a small share of the
/// index can sit entirely beyond it. That got worse as the index grew.
///
/// Measured on prod (2026-10-15, 1.65M vectors, 19 scopes): the filtered ANN
/// scan's recall@10 against exact was 0.1 to 0.3 on code scopes and 0.3 to
/// 0.77 on 90k to 300k-source scopes, even on full pages. Exact cost 18 to 31
/// µs per source warm (about 1.7 s at 100k, 3 s at 150k). Ruled (Jordan): no
/// missed results up to 150k. Override per database or session with
/// `fs3.exact_scope_limit`.
pub const EXACT_SCOPE_SOURCES: i64 = 150_000;

/// The largest scope re-ranked exactly after its ANN page came back short.
///
/// The iterative scan stopping at `hnsw.max_scan_tuples` with the candidate
/// window unfilled says the scope sits beyond what the scan visits, not that
/// it ran out. Its few hits are then not the scope's nearest: on prod
/// (2026-10-14) a 12,053-source code scope matched 3 to 43 of 101 hits, and
/// recall@10 against exact was 0.14. A larger scope reports the page as
/// incomplete instead of paying for an exact pass. The same ceiling as
/// [`EXACT_SCOPE_SOURCES`], so it only acts where `fs3.exact_scope_limit`
/// sets a lower one.
pub const EXACT_FALLBACK_SOURCES: i64 = EXACT_SCOPE_SOURCES;

/// Start with enough vectors for ordinary two- or three-chunk elements while
/// keeping the common ANN scan small.
const INITIAL_CANDIDATE_MULTIPLIER: i64 = 4;
/// Double only when the current candidate page is full and still under-fills
/// the requested number of distinct elements.
const CANDIDATE_GROWTH_FACTOR: i64 = 2;
/// Bound both search paths after eight retries. The unfiltered
/// [`query_embeddings`] path refuses an unreachable under-fill; filtered
/// [`search_elements`] returns a short page with explicit scan metadata.
const MAX_CANDIDATE_EXPANSIONS: usize = 8;

fn candidate_count(rows: &[sqlx::postgres::PgRow]) -> Result<i64, StoreError> {
    rows.first()
        .map(|row| row.try_get("candidate_count"))
        .transpose()
        .map(|count| count.unwrap_or(0))
        .map_err(StoreError::from)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExpansionDecision {
    Continue,
    /// Run the same window again, ranking the scope exactly.
    RankExactly,
    Return {
        scan_incomplete: bool,
    },
}

/// What one candidate pass found, as [`expansion_decision`] reads it.
#[derive(Clone, Copy, Debug, Default)]
struct Pass {
    hit_count: i64,
    requested: i64,
    scanned: i64,
    candidate_limit: i64,
    admitted: i64,
    previous_admitted: Option<i64>,
    expansion: usize,
    /// The scope was ranked exactly.
    exact: bool,
    /// The search names a scope, so it can be ranked exactly.
    scoped: bool,
    /// Distinct sources the scope admits.
    scope_sources: i64,
    /// The index holds more vectors than the ANN scan may visit.
    scan_capped: bool,
    /// A distance floor is set, which can shorten a page on its own.
    floored: bool,
}

fn expansion_decision(pass: Pass) -> ExpansionDecision {
    let admitted_stalled = pass.previous_admitted == Some(pass.admitted);
    let short = pass.scanned < pass.candidate_limit;
    let expansion_exhausted = admitted_stalled || pass.expansion == MAX_CANDIDATE_EXPANSIONS;
    // A short ANN page is the scan budget running out, not the scope, unless
    // the scan could visit the whole index.
    let starved = short && !pass.exact && pass.scan_capped;
    if pass.hit_count >= pass.requested {
        ExpansionDecision::Return {
            scan_incomplete: false,
        }
    } else if starved {
        // The retry is a pass of its own, so the last pass cannot take it.
        if pass.scoped
            && pass.scope_sources <= EXACT_FALLBACK_SOURCES
            && pass.expansion < MAX_CANDIDATE_EXPANSIONS
        {
            ExpansionDecision::RankExactly
        } else {
            // Under a floor, the floor is the likelier cut and is reported.
            ExpansionDecision::Return {
                scan_incomplete: !pass.floored,
            }
        }
    } else if short || expansion_exhausted {
        ExpansionDecision::Return {
            scan_incomplete: expansion_exhausted && !short,
        }
    } else {
        ExpansionDecision::Continue
    }
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
    /// Every variant. Its spelling is also a semantic hit's `match_field`, which
    /// the bundled agent skill is checked against
    /// (`crates/cli/tests/skill_accuracy.rs`).
    pub const ALL: [Self; 2] = [SourceKind::Raw, SourceKind::Smart];

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
    /// Turn classes left out of the results. `None` hides nothing.
    ///
    /// The daemon defaults this to [`fs3_core::TurnClass::HIDDEN_BY_DEFAULT`]
    /// — fs3's own retrieval traffic — and a caller widens it explicitly. An
    /// unclassified turn (`turn_class IS NULL`) is never hidden.
    pub hidden_turn_classes: Option<Vec<fs3_core::TurnClass>>,
    /// How many hits to return.
    pub limit: i64,
    /// How long this leg may take, for a leg whose answer is advice rather
    /// than the page (the "other repositories" hint).
    ///
    /// Each statement runs under this `statement_timeout`, and the vector leg
    /// starts no further candidate pass once it is spent. `None` is no limit.
    pub budget: Option<std::time::Duration>,
    /// Which index answers the keyword leg. The daemon chooses BM25 only once
    /// [`crate::bm25_index_ready`] said the index can serve; a query BM25
    /// cannot answer still goes to trigram (see [`crate::bm25_can_answer`]).
    pub lexical_backend: crate::LexicalBackend,
    /// The parser version that answers for a blob that has rows under it.
    ///
    /// A blob keeps every parser version's rows, so without this rule both
    /// versions of one declaration are candidates, and after a bump that moved
    /// its span (DL-042) they surface as two hits. This is the rule `get` and
    /// `tree` already follow: the current version when the blob has it, and
    /// otherwise whatever it has, so nothing goes dark while a reparse is
    /// pending. Turns are written under the conversation parser and never have
    /// code-version rows, so they always pass. `None` applies no rule.
    pub parser_version: Option<String>,
    /// Read the scope from the scope cache (`search.scope_cache`). Off by
    /// default, so a caller opts in; see [`crate::scope_cache`].
    pub scope_cache: bool,
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
            hidden_turn_classes: None,
            limit: 10,
            budget: None,
            lexical_backend: crate::LexicalBackend::Trigram,
            parser_version: None,
            scope_cache: false,
        }
    }
}

impl SearchFilters {
    /// The hidden classes in their storage spelling, as one SQL array bind.
    pub(crate) fn hidden_turn_class_names(&self) -> Option<Vec<&'static str>> {
        self.hidden_turn_classes
            .as_ref()
            .map(|classes| classes.iter().map(|class| class.as_str()).collect())
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
/// One completed search page and how the bounded expansion reached it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SearchPage {
    /// Caller-visible hits, nearest first.
    pub hits: Vec<SearchHit>,
    /// Number of SQL candidate pages examined.
    pub passes: usize,
    /// The scan returned a short page because admitted candidates stopped
    /// growing or the configured expansion ceiling was reached.
    pub candidate_limit_exhausted: bool,
    /// Distinct contents the turn-class filter removed from this page's
    /// candidates — what a disclosure reports as hidden.
    pub hidden_turns: u64,
    /// The scope was ranked exactly and every one of its vectors was within
    /// the candidate window, so this page is complete: a short or empty one
    /// means the scope holds nothing more that matches.
    pub exact_scan: bool,
    /// Milliseconds spent getting a pooled connection and opening the search
    /// transaction. The pool is shared with every background worker, so a
    /// slow search can be a queue for a connection rather than a slow query.
    pub acquire_ms: u64,
    /// Milliseconds each candidate pass took, in order. The scope is rebuilt
    /// on every pass, so the passes, not one bad statement, are often what a
    /// slow semantic leg is made of.
    pub pass_ms: Vec<u64>,
    /// The vector candidate limit of the last pass.
    pub candidates: i64,
    /// How the scope was come by: read from the cache, built into it, or
    /// collected in full.
    pub scope_cache: crate::ScopeCacheUse,
    /// Milliseconds spent checking (and, on a miss, building) the cached
    /// scope, before the first pass.
    pub scope_cache_ms: u64,
}

fn page_from_rows(
    rows: &[sqlx::postgres::PgRow],
    passes: usize,
    candidate_limit_exhausted: bool,
    exact_scan: bool,
) -> Result<SearchPage, StoreError> {
    let hits = rows
        .iter()
        .map(|row| {
            if row.try_get::<Option<i64>, _>("element_id")?.is_none() {
                return Ok(None);
            }
            Ok(Some(SearchHit {
                similar: similar_from_row(row)?,
                identity: row.try_get("identity")?,
                root_path: row.try_get("root_path")?,
                path: row.try_get("path")?,
            }))
        })
        .filter_map(Result::transpose)
        .collect::<Result<Vec<_>, StoreError>>()?;
    let hidden_turns = rows
        .first()
        .map(|row| row.try_get::<i64, _>("hidden_count"))
        .transpose()?
        .map_or(0, |count| u64::try_from(count).unwrap_or(0));
    Ok(SearchPage {
        hits,
        passes,
        candidate_limit_exhausted,
        hidden_turns,
        exact_scan,
        ..SearchPage::default()
    })
}

#[cfg(test)]
const ADMISSION_JOIN_SQL: &str = r#"JOIN page_admitted_sources admitted
        ON admitted.source_hash = candidate.source_hash
       AND admitted.source_kind = candidate.source_kind"#;

/// The CTEs that collect a scope's admitted sources, ending in
/// `admitted_sources`. Shared by [`SEARCH_ELEMENTS_SQL`] and the scope cache's
/// build (`crate::scope_cache`), so the cache holds exactly what a search would
/// have collected. `$19` names a cached scope and `$20` its cutoff; NULL, or a
/// key with no `scope_cache` row, collects the scope in full.
macro_rules! scope_ctes {
    () => {
        r#"scope_cached AS MATERIALIZED (
    -- The cached scope ($19). No row (no key, or one dropped since the search
    -- looked) collects the scope in full. Its delta reads rows stamped by the
    -- transactions its build's snapshot could not see: xid $20 (that
    -- snapshot's xmin) or later. Bound as a value, so a custom plan sees how
    -- recent it is and starts each delta from its stamp index. An older cutoff
    -- than the row's (a rebuild in between) only reads more.
    SELECT 1 FROM scope_cache WHERE scope_key = $19
),
scope_file_blobs AS MATERIALIZED (
    SELECT f.blob_sha
      FROM worktree_files f
      JOIN worktrees w ON w.id = f.worktree_id
      JOIN repos r     ON r.id = w.repo_id
     WHERE ($6::text IS NOT NULL OR $7::text IS NOT NULL OR $9::text IS NOT NULL)
       AND ($6::text IS NULL OR r.identity = $6)
       AND ($7::text IS NULL OR f.path LIKE $7)
       AND ($9::text IS NULL OR w.root_path = $9)
       -- With a cached scope, only what changed since its cutoff: a path
       -- added or re-pointed, or a blob that gained elements.
       AND (NOT EXISTS (SELECT 1 FROM scope_cached)
            OR f.changed_xid >= $20::xid8
            OR f.blob_sha IN (SELECT changed.blob_sha
                                FROM elements changed
                               WHERE changed.changed_xid >= $20::xid8))
),
-- A turn's element carries the turn's blob as its raw hash (one turn, one
-- element: `write_turn_elements`), so a turn in scope IS a raw source, and
-- looking each one up in `elements` only to read that hash back was most of
-- what a scope cost: on prod (2026-10-14) 98% of the flowspace3 scope's
-- blobs were turns, and the lookups were 380 ms of 858 ms warm. The filters
-- an element lookup applied still hold: only when turns are an admitted kind,
-- and never under a ddoc filter, which no turn carries.
scope_turn_sources AS MATERIALIZED (
    SELECT t.blob_sha AS raw_hash
      FROM turns t
      JOIN conversations c ON c.guid = t.conversation_id
     WHERE ($6::text IS NOT NULL OR $7::text IS NOT NULL
            OR $9::text IS NOT NULL OR $13::text IS NOT NULL)
       AND ($8::text[] IS NULL OR 'turn' = ANY($8))
       AND $10::text[] IS NULL
       AND $12::text IS NULL
       AND ($6::text IS NULL OR c.repo_identity = $6)
       AND ($7::text IS NULL OR c.worktree LIKE $7)
       AND ($9::text IS NULL OR c.worktree IS NULL OR c.worktree = $9)
       AND ($13::text IS NULL OR c.guid = $13::uuid)
       -- With a cached scope, only conversations stamped since its cutoff:
       -- new turns (a deduped one too), a restatement, or a repository or
       -- worktree set later. So the caller's own live session is always read.
       AND (NOT EXISTS (SELECT 1 FROM scope_cached)
            OR c.changed_xid >= $20::xid8)
),
-- The content filters apply here as well as at admission. A scope's nearest
-- vectors can all be of the wrong kind (a repository known mostly through
-- conversations, searched with `--source doc`), and filtering only after the
-- candidate LIMIT stalled the expansion before it reached content that matched.
scope_representatives AS MATERIALIZED (
    SELECT scoped.raw_hash
      FROM elements scoped
      JOIN scope_file_blobs scope ON scope.blob_sha = scoped.blob_sha
     WHERE ($13::text IS NULL OR strpos(scoped.address, 'conv:' || $13 || '#t') = 1)
       AND ($8::text[] IS NULL OR scoped.kind = ANY($8))
       AND ($10::text[] IS NULL OR scoped.ddoc->>'id_kind' = ANY($10))
       AND ($12::text IS NULL OR scoped.ddoc->>'schema' = $12)
       -- One parser version per blob ($18): the current one when the blob
       -- has it, else whatever it has (the rule `get` follows).
       AND ($18::text IS NULL OR scoped.parser_version = $18
            OR NOT EXISTS (SELECT 1 FROM elements current_parse
                            WHERE current_parse.blob_sha = scoped.blob_sha
                              AND current_parse.parser_version = $18))
    UNION
    SELECT raw_hash FROM scope_turn_sources
),
-- Distinct sources, so `scope_size` counts each once. The cached rows are
-- distinct already (their key): they are appended whole, and only the small
-- set collected beside them is deduplicated and checked against them, rather
-- than hashing the whole cached scope again on every pass.
--
-- Summaries written since the cutoff for a raw source the cache holds are
-- their own CTE: that raw source may have left the scope since, so each one
-- may be departed, and `scope_size` counts them as such.
scope_late_summaries AS MATERIALIZED (
    SELECT smart.text_hash AS source_hash
      FROM smart_content smart
     WHERE smart.changed_xid >= $20::xid8
       AND EXISTS (SELECT 1
                     FROM scope_sources cached
                    WHERE cached.scope_key = $19
                      AND cached.source_kind = 'raw'
                      AND cached.source_hash = smart.raw_hash)
),
admitted_sources AS MATERIALIZED (
    SELECT cached.source_hash, cached.source_kind
      FROM scope_sources cached
     WHERE cached.scope_key = $19
       AND EXISTS (SELECT 1 FROM scope_cached)
    UNION ALL
    SELECT collected.source_hash, collected.source_kind
      FROM (
        SELECT raw_hash AS source_hash, 'raw'::text AS source_kind
          FROM scope_representatives
        UNION
        SELECT smart.text_hash, 'smart'::text
          FROM smart_content smart
          JOIN scope_representatives scoped ON scoped.raw_hash = smart.raw_hash
        UNION
        SELECT source_hash, 'smart'::text FROM scope_late_summaries
      ) collected
     -- Uncached, nothing to check against: the first test short-circuits.
     WHERE NOT EXISTS (SELECT 1 FROM scope_cached)
        OR NOT EXISTS (SELECT 1
                         FROM scope_sources cached
                        WHERE cached.scope_key = $19
                          AND cached.source_kind = collected.source_kind
                          AND cached.source_hash = collected.source_hash)
)"#
    };
}
pub(crate) use scope_ctes;

const SEARCH_ELEMENTS_SQL: &str = concat!(
    "WITH ",
    scope_ctes!(),
    r#",
-- Summed here rather than when the key was checked, so a departure that
-- commits in between is counted too.
scope_departed AS MATERIALIZED (
    SELECT (SELECT COALESCE(sum(s.removed), 0)::bigint
              FROM scope_cache c
              JOIN scope_shrinks s
                ON (c.watch IS NULL OR s.identity = ANY(c.watch))
               AND s.xid >= c.cutoff
             WHERE c.scope_key = $19)
           + (SELECT count(*) FROM scope_late_summaries) AS departed
),
scope_size AS MATERIALIZED (
    -- $17 is the fallback ceiling: set only on a pass that follows an ANN
    -- page the scan budget cut short, which this scope is re-ranked exactly
    -- after when it is no larger than that. Unset, it must read false, not
    -- NULL: a NULL `exact` would pass neither arm and return nothing.
    -- A cached scope's count can overstate the live one by its departed
    -- sources: the departures logged since its cutoff and any late summary
    -- of a raw source that has left (`scope_departed`). So a scope within that many of
    -- a limit is ranked exactly - exact over a superset is never worse than
    -- the live scope's own path.
    SELECT ($6::text IS NOT NULL OR $7::text IS NOT NULL
            OR $9::text IS NOT NULL OR $13::text IS NOT NULL)
           AND (count(*) <= COALESCE(
                    NULLIF(current_setting('fs3.exact_scope_limit', true), '')::bigint,
                    $16) + (SELECT departed FROM scope_departed)
                OR ($17::bigint IS NOT NULL
                    AND count(*) <= $17 + (SELECT departed FROM scope_departed))) AS exact,
           count(*)::bigint AS sources
      FROM admitted_sources
),
candidate_vectors AS MATERIALIZED (
    -- A small scope is ranked exactly: every one of its vectors is compared,
    -- and the true nearest $14 are kept. The ANN scan below gives up after
    -- `hnsw.max_scan_tuples`, and a scope that is a small share of the index
    -- can sit past that. Each scope source is one primary-key lookup:
    -- `OFFSET 0` keeps the lateral from being flattened into a join, which a
    -- custom plan ran as a merge join over the whole key (1.65M entries for a
    -- 12,784-source scope, 4 s instead of 0.3 s on prod, 2026-10-15).
    -- Exactly one arm runs, so the outer LIMIT only tells the planner that.
    SELECT * FROM (
    (SELECT e.source_hash, e.source_kind, e.chunk_no, e.distance
      FROM admitted_sources a
      CROSS JOIN LATERAL (
           SELECT v.source_hash, v.source_kind, v.chunk_no,
                  v.vector <=> $1 AS distance
             FROM embeddings_1024 v
            WHERE v.source_hash = a.source_hash
              AND v.source_kind = a.source_kind
              AND v.model_key = $2
           OFFSET 0) e
     WHERE (SELECT exact FROM scope_size)
       AND ($4::text IS NULL OR e.source_kind = $4)
       AND ($5::float8 IS NULL OR e.distance <= $5)
     ORDER BY e.distance
     LIMIT $14)
    UNION ALL
    (SELECT e.source_hash, e.source_kind, e.chunk_no,
           e.vector <=> $1 AS distance
      FROM embeddings_1024 e
     WHERE NOT (SELECT exact FROM scope_size)
       AND e.model_key = $2
       AND ($4::text IS NULL OR e.source_kind = $4)
       AND ($5::float8 IS NULL OR (e.vector <=> $1) <= $5)
       AND (($6::text IS NULL AND $7::text IS NULL
             AND $9::text IS NULL AND $13::text IS NULL)
            OR (e.source_kind = 'raw' AND e.source_hash IN (
                 SELECT source_hash FROM admitted_sources WHERE source_kind = 'raw'))
            OR (e.source_kind = 'smart' AND e.source_hash IN (
                 SELECT source_hash FROM admitted_sources WHERE source_kind = 'smart')))
     ORDER BY e.vector <=> $1
     LIMIT $14)
    ) arms
    LIMIT $14
),
candidate_meta AS (
    -- `scan_capped`: the index holds more vectors than the iterative ANN scan
    -- may visit, so a short ANN page can mean the scan stopped, not that the
    -- scope ran out. A table never analyzed reads -1 and counts as small.
    SELECT count(*)::bigint AS candidate_count,
           (SELECT exact FROM scope_size) AS exact_scan,
           (SELECT sources FROM scope_size) AS scope_sources,
           (SELECT departed FROM scope_departed) AS scope_departed,
           (SELECT reltuples FROM pg_class WHERE oid = 'embeddings_1024'::regclass)
               > COALESCE(NULLIF(current_setting('hnsw.max_scan_tuples', true), '')::float8,
                          20000) AS scan_capped
      FROM candidate_vectors
),
candidate_raw_hashes AS MATERIALIZED (
    SELECT source_hash AS raw_hash
      FROM candidate_vectors
     WHERE source_kind = 'raw'
    UNION
    SELECT smart.raw_hash
      FROM candidate_vectors candidate
      JOIN smart_content smart ON smart.text_hash = candidate.source_hash
     WHERE candidate.source_kind = 'smart'
),
admitted_scoped AS MATERIALIZED (
    SELECT admitted.id, admitted.raw_hash,
           COALESCE(admitted.turn_class = ANY($15::text[]), FALSE) AS hidden
      FROM elements admitted
      JOIN candidate_raw_hashes page ON page.raw_hash = admitted.raw_hash
     WHERE ($8::text[] IS NULL OR admitted.kind = ANY($8))
       AND ($10::text[] IS NULL OR admitted.ddoc->>'id_kind' = ANY($10))
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
       AND ($12::text IS NULL OR admitted.ddoc->>'schema' = $12)
       AND ($13::text IS NULL OR strpos(admitted.address, 'conv:' || $13 || '#t') = 1)
       -- Repeated here, not only at scope collection: an unscoped search has
       -- no scope CTE, and a raw hash admitted through another blob must not
       -- resolve to this blob's superseded row.
       AND ($18::text IS NULL OR admitted.parser_version = $18
            OR NOT EXISTS (SELECT 1 FROM elements current_parse
                            WHERE current_parse.blob_sha = admitted.blob_sha
                              AND current_parse.parser_version = $18))
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
admitted_elements AS MATERIALIZED (
    SELECT id, raw_hash
      FROM admitted_scoped
     WHERE NOT hidden
),
-- What the turn-class filter took out of this page's candidates: content that
-- would have been admitted and has no visible occurrence left. Counted in the
-- same pass so disclosure costs no second query.
hidden_meta AS (
    SELECT count(DISTINCT hidden.raw_hash)::bigint AS hidden_count
      FROM admitted_scoped hidden
     WHERE hidden.hidden
       AND NOT EXISTS (SELECT 1 FROM admitted_elements visible
                        WHERE visible.raw_hash = hidden.raw_hash)
),
admitted_representatives AS MATERIALIZED (
    SELECT DISTINCT ON (raw_hash) id, raw_hash
      FROM admitted_elements
     ORDER BY raw_hash, id
),
smart_map AS MATERIALIZED (
    SELECT smart.raw_hash, smart.model_key, smart.text,
           smart.text_hash, smart.tags, smart.extras, smart.created_at
      FROM smart_content smart
      JOIN candidate_vectors candidate
        ON candidate.source_kind = 'smart' AND candidate.source_hash = smart.text_hash
      JOIN admitted_representatives admitted ON admitted.raw_hash = smart.raw_hash
),
smart_representatives AS MATERIALIZED (
    SELECT DISTINCT ON (text_hash) raw_hash, text_hash, text, tags, extras
      FROM smart_map
     ORDER BY text_hash, created_at, model_key, raw_hash
),
page_admitted_sources AS MATERIALIZED (
    SELECT raw_hash AS source_hash, 'raw'::text AS source_kind
      FROM admitted_representatives
    UNION
    SELECT text_hash AS source_hash, 'smart'::text AS source_kind
      FROM smart_representatives
),
admitted_candidates AS MATERIALIZED (
    SELECT candidate.*
      FROM candidate_vectors candidate
      JOIN page_admitted_sources admitted
        ON admitted.source_hash = candidate.source_hash
       AND admitted.source_kind = candidate.source_kind
),
admitted_meta AS (
    SELECT count(*)::bigint AS admitted_count
      FROM admitted_candidates
),
resolved AS (
    SELECT c.source_hash, c.source_kind, c.chunk_no, c.distance,
           s.text, s.tags, s.extras,
           e.id AS element_id,
           e.blob_sha, e.parser_version, e.kind, e.subkind, e.name,
           e.address, e.span_start, e.span_end, e.sibling_order,
           e.raw_text, e.ddoc
      FROM admitted_candidates c
      LEFT JOIN smart_representatives s
        ON c.source_kind = 'smart' AND s.text_hash = c.source_hash
      JOIN admitted_representatives admitted
        ON admitted.raw_hash = COALESCE(s.raw_hash, c.source_hash)
      JOIN elements e ON e.id = admitted.id
),
nearest AS (
    SELECT DISTINCT ON (element_id) *
      FROM resolved
     ORDER BY element_id, distance, source_kind, source_hash, chunk_no
),
final_hits AS (
    SELECT n.element_id, n.source_kind, n.distance,
           n.text, n.tags, n.extras,
           n.blob_sha, n.parser_version, n.kind, n.subkind, n.name,
           n.address, n.span_start, n.span_end, n.sibling_order, n.raw_text,
           n.ddoc,
           COALESCE(live.identity, anchored.identity) AS identity,
           COALESCE(live.root_path, anchored.root_path) AS root_path,
           live.path
      FROM nearest n
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
     LIMIT $3
)
SELECT h.source_kind, h.distance,
       h.text, h.tags, h.extras,
       h.blob_sha, h.parser_version, h.kind, h.subkind, h.name,
       h.address, h.span_start, h.span_end, h.sibling_order, h.raw_text,
       h.ddoc, h.identity, h.root_path, h.path,
       candidate.candidate_count, candidate.exact_scan,
       candidate.scope_sources, candidate.scope_departed, candidate.scan_capped,
       admitted.admitted_count, hidden.hidden_count,
       h.element_id
  FROM candidate_meta candidate
  CROSS JOIN admitted_meta admitted
  CROSS JOIN hidden_meta hidden
  LEFT JOIN final_hits h ON TRUE
 ORDER BY h.distance, h.element_id"#
);

/// The `limit` nearest elements to `query`, narrowed by `filters`, nearest first.
///
/// Scope source keys are resolved once and applied inside the ordered HNSW scan,
/// so iterative scan can pass nearer foreign vectors without probing
/// `smart_content` per candidate. Full smart-content and element resolution is
/// bounded to the resulting candidate page.
///
/// Candidate expansion is deliberately bounded. A short page is successful:
/// [`SearchPage::candidate_limit_exhausted`] distinguishes admitted-candidate
/// stagnation or the ceiling from ordinary index exhaustion without turning a
/// diagnostic search into an outage.
///
/// # Errors
/// [`StoreError::Dimensions`] when `query` is the wrong width;
/// [`StoreError::Query`] on database failure; [`StoreError::Corrupt`] when a row
/// carries an unknown kind.
pub async fn search_elements(
    pool: &PgPool,
    model_key: &str,
    query: &[f32],
    filters: &SearchFilters,
) -> Result<SearchPage, StoreError> {
    if query.len() != EMBEDDING_DIMENSIONS {
        return Err(StoreError::Dimensions {
            expected: EMBEDDING_DIMENSIONS,
            actual: query.len(),
        });
    }

    let cache_started = std::time::Instant::now();
    let (scope, scope_cache) = crate::cached_scope(pool, filters).await;
    let scope_cache_ms = crate::elapsed_ms(cache_started);

    let acquire_started = std::time::Instant::now();
    let mut tx = pool.begin().await?;
    configure_search_transaction(&mut tx).await?;
    apply_budget(&mut tx, filters.budget).await?;
    let acquire_ms = crate::elapsed_ms(acquire_started);
    let mut pass_ms = Vec::new();

    // One statement text for every filter combination. Bind map: $1 vector,
    // $2 model, $3 element limit, $4 source, $5 distance, $6 repo, $7 path,
    // $8 kinds, $9 worktree, $10 id_kinds, $11 gate_open, $12 ddoc_schema,
    // $13 conversation, $14 vector candidate limit, $15 hidden turn classes,
    // $16 the default exact-scan ceiling, $17 the fallback ceiling (only on a
    // pass after a short ANN page), $18 the parser version that answers for a
    // blob that has it, $19 the cached scope's key and $20 its cutoff (NULL:
    // the scope is collected in full).
    let scoped = filters.repo.is_some()
        || filters.path.is_some()
        || filters.worktree.is_some()
        || filters.conversation.is_some();
    let mut candidate_limit = filters.limit.saturating_mul(INITIAL_CANDIDATE_MULTIPLIER);
    let mut previous_admitted = None;
    let mut fallback = None;
    for expansion in 0..=MAX_CANDIDATE_EXPANSIONS {
        let pass_started = std::time::Instant::now();
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
            .bind(filters.hidden_turn_class_names())
            .bind(EXACT_SCOPE_SOURCES)
            .bind(fallback)
            .bind(filters.parser_version.as_deref())
            .bind(scope.as_ref().map(|cached| cached.key.as_str()))
            .bind(scope.as_ref().map(|cached| cached.cutoff.as_str()))
            .fetch_all(&mut *tx)
            .await?;
        pass_ms.push(crate::elapsed_ms(pass_started));

        let passes = expansion + 1;
        let exact = rows
            .first()
            .map(|row| row.try_get::<bool, _>("exact_scan"))
            .transpose()?
            .unwrap_or(false);
        let scanned = candidate_count(&rows)?;
        let scope_sources: i64 = rows
            .first()
            .map(|row| row.try_get("scope_sources"))
            .transpose()?
            .unwrap_or(0);
        let scope_departed: i64 = rows
            .first()
            .map(|row| row.try_get("scope_departed"))
            .transpose()?
            .unwrap_or(0);
        let scan_capped: bool = rows
            .first()
            .map(|row| row.try_get::<Option<bool>, _>("scan_capped"))
            .transpose()?
            .flatten()
            .unwrap_or(false);
        let admitted: i64 = rows
            .first()
            .map(|row| row.try_get("admitted_count"))
            .transpose()?
            .unwrap_or(0);
        let hit_count = rows.iter().try_fold(0_i64, |count, row| {
            row.try_get::<Option<i64>, _>("element_id")
                .map(|element_id| count + i64::from(element_id.is_some()))
                .map_err(StoreError::from)
        })?;
        // A budgeted leg answers with what it has once its time is spent,
        // reported as an incomplete scan rather than a failure.
        let spent = filters
            .budget
            .is_some_and(|budget| acquire_started.elapsed() >= budget);
        let decision = match expansion_decision(Pass {
            hit_count,
            requested: filters.limit,
            scanned,
            candidate_limit,
            admitted,
            previous_admitted,
            expansion,
            exact,
            // One exact retry: its own page decides from there.
            scoped: scoped && fallback.is_none(),
            // Never count departed sources against the fallback ceiling.
            scope_sources: scope_sources - scope_departed,
            scan_capped,
            floored: filters.max_distance.is_some(),
        }) {
            ExpansionDecision::Continue | ExpansionDecision::RankExactly if spent => {
                ExpansionDecision::Return {
                    scan_incomplete: true,
                }
            }
            decision => decision,
        };
        match decision {
            ExpansionDecision::Return { scan_incomplete } => {
                tx.commit().await?;
                // Exact AND the window held the whole scope: nothing in it
                // was left unranked, so a short page is complete.
                let exact_scan = exact && scanned < candidate_limit;
                return page_from_rows(&rows, passes, scan_incomplete, exact_scan).map(|page| {
                    SearchPage {
                        acquire_ms,
                        pass_ms,
                        candidates: candidate_limit,
                        scope_cache,
                        scope_cache_ms,
                        ..page
                    }
                });
            }
            ExpansionDecision::RankExactly => {
                fallback = Some(EXACT_FALLBACK_SOURCES);
                continue;
            }
            ExpansionDecision::Continue => {}
        }
        previous_admitted = Some(admitted);
        candidate_limit = candidate_limit.saturating_mul(CANDIDATE_GROWTH_FACTOR);
    }

    unreachable!("bounded candidate expansion returns on its final pass")
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

    #[test]
    fn filtered_search_returns_a_short_page_at_the_expansion_bound() {
        assert_eq!(
            expansion_decision(Pass {
                hit_count: 1,
                requested: 10,
                scanned: 10_240,
                candidate_limit: 10_240,
                admitted: 9,
                previous_admitted: Some(8),
                expansion: MAX_CANDIDATE_EXPANSIONS,
                ..Pass::default()
            }),
            ExpansionDecision::Return {
                scan_incomplete: true,
            }
        );

        assert_eq!(
            expansion_decision(Pass {
                hit_count: 1,
                requested: 10,
                scanned: 100,
                candidate_limit: 160,
                admitted: 5,
                previous_admitted: Some(5),
                expansion: 3,
                ..Pass::default()
            }),
            ExpansionDecision::Return {
                scan_incomplete: false,
            }
        );
    }

    /// prod, 2026-10-14: a 12,053-source scope's ANN page held 3 to 43 of
    /// 101 hits in one pass, and was reported complete.
    #[test]
    fn a_short_ann_page_on_a_capped_index_is_never_complete() {
        let starved = Pass {
            hit_count: 12,
            requested: 101,
            scanned: 30,
            candidate_limit: 404,
            admitted: 12,
            expansion: 0,
            scoped: true,
            scope_sources: 12_053,
            scan_capped: true,
            ..Pass::default()
        };
        assert_eq!(
            expansion_decision(starved),
            ExpansionDecision::RankExactly,
            "a small scope is ranked exactly instead"
        );
        assert_eq!(
            expansion_decision(Pass {
                scope_sources: EXACT_FALLBACK_SOURCES + 1,
                ..starved
            }),
            ExpansionDecision::Return {
                scan_incomplete: true,
            },
            "a scope too large to rank exactly says the scan stopped"
        );
        assert_eq!(
            expansion_decision(Pass {
                scoped: false,
                ..starved
            }),
            ExpansionDecision::Return {
                scan_incomplete: true,
            },
            "an unscoped page cut short says so too"
        );
        assert_eq!(
            expansion_decision(Pass {
                expansion: MAX_CANDIDATE_EXPANSIONS,
                ..starved
            }),
            ExpansionDecision::Return {
                scan_incomplete: true,
            },
            "the last pass has no retry left"
        );
        // The exact pass's own short page is the scope running out.
        assert_eq!(
            expansion_decision(Pass {
                exact: true,
                ..starved
            }),
            ExpansionDecision::Return {
                scan_incomplete: false,
            }
        );
        assert_eq!(
            expansion_decision(Pass {
                floored: true,
                ..starved
            }),
            ExpansionDecision::RankExactly,
            "a floor does not stop a small scope being ranked exactly"
        );
        // An index the scan visits in full, or a floor, shortens a page honestly.
        for honest in [
            Pass {
                scan_capped: false,
                ..starved
            },
            Pass {
                floored: true,
                scoped: false,
                ..starved
            },
        ] {
            assert_eq!(
                expansion_decision(honest),
                ExpansionDecision::Return {
                    scan_incomplete: false,
                }
            );
        }
    }

    const SHAPE_MODEL: &str = "search-plan-shape@1024";

    async fn shape_database() -> (String, PgPool, PgPool) {
        use std::sync::atomic::{AtomicU64, Ordering};
        use std::time::{SystemTime, UNIX_EPOCH};

        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let base_url = fs3_testkit::test_database_url();
        let (maintenance_url, _) = crate::maintenance_url(&base_url).unwrap();
        let admin = crate::connect(&maintenance_url).await.unwrap();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let name = format!("fs3_test_searchplan_{nanos}_{sequence}");
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
        .bind(Vector::from(query.clone()))
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO embeddings_1024
                 (source_hash, source_kind, chunk_no, model_key, vector, truncated)
             SELECT raw_hash, 'raw', 0, $1, $2, false
               FROM elements
              ORDER BY id
              LIMIT 10000",
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
        // The last distance filter is the HNSW arm's; the exact arm's comes first.
        let (at, _) = without_join
            .match_indices(distance_filter)
            .last()
            .expect("distance filter marker drifted");
        let at = at + distance_filter.len();
        let old = format!(
            "{}\n{old_admission}{}",
            &without_join[..at],
            &without_join[at..]
        );
        assert_ne!(old, without_join, "distance filter marker drifted");
        old
    }

    async fn explain_search(
        connection: &mut sqlx::PgConnection,
        sql: &str,
        analyze: bool,
    ) -> Result<serde_json::Value, sqlx::Error> {
        let options = if analyze {
            "ANALYZE, BUFFERS, VERBOSE, FORMAT JSON"
        } else {
            "VERBOSE, FORMAT JSON"
        };
        let statement = format!("EXPLAIN ({options}) {sql}");
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
            // The daemon's default: retrieval echoes hidden. The plan is
            // checked with the filter the shipped search actually runs.
            .bind(Some(
                fs3_core::TurnClass::HIDDEN_BY_DEFAULT
                    .iter()
                    .map(|class| class.as_str())
                    .collect::<Vec<_>>(),
            ))
            .bind(EXACT_SCOPE_SOURCES)
            .bind(Option::<i64>::None)
            // The daemon always names its parser; the corpus's is current.
            .bind(Some("search-plan@1"))
            .bind(Option::<&str>::None)
            .bind(Option::<&str>::None)
            .fetch_one(&mut *connection)
            .await?;
        Ok(row.try_get::<Json<serde_json::Value>, _>(0)?.0)
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
                && let Some(actual) = node.get("Actual Loops").and_then(serde_json::Value::as_f64)
            {
                loops = Some(loops.map_or(actual, |current| current.max(actual)));
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

    fn candidate_vector_rows(plan: &serde_json::Value) -> Option<(f64, Option<f64>)> {
        let mut rows = None;
        visit_plan(plan, &mut |node| {
            if node.get("Subplan Name").and_then(serde_json::Value::as_str)
                == Some("CTE candidate_vectors")
            {
                rows = node
                    .get("Plan Rows")
                    .and_then(serde_json::Value::as_f64)
                    .map(|estimated| {
                        (
                            estimated,
                            node.get("Actual Rows").and_then(serde_json::Value::as_f64),
                        )
                    });
            }
        });
        rows
    }

    #[derive(Debug)]
    struct VectorDriver {
        actual_rows: Option<f64>,
        ordered_by_distance: bool,
        child_count: usize,
    }

    fn vector_driver(plan: &serde_json::Value) -> Option<VectorDriver> {
        let mut driver = None;
        visit_plan(plan, &mut |node| {
            if node.get("Index Name").and_then(serde_json::Value::as_str)
                == Some("embeddings_1024_vector_idx")
            {
                driver = Some(VectorDriver {
                    actual_rows: node.get("Actual Rows").and_then(serde_json::Value::as_f64),
                    ordered_by_distance: node
                        .get("Order By")
                        .is_some_and(|order| order.to_string().contains("<=>")),
                    child_count: node
                        .get("Plans")
                        .and_then(serde_json::Value::as_array)
                        .map_or(0, Vec::len),
                });
            }
        });
        driver
    }

    fn has_correlated_smart_subplan(plan: &serde_json::Value) -> bool {
        let mut found = false;
        visit_plan(plan, &mut |node| {
            let smart_content = node
                .get("Relation Name")
                .and_then(serde_json::Value::as_str)
                == Some("smart_content");
            let subplan = node
                .get("Parent Relationship")
                .and_then(serde_json::Value::as_str)
                == Some("SubPlan")
                || node
                    .get("Subplan Name")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|name| name.starts_with("SubPlan"));
            found |= smart_content && subplan;
        });
        found
    }

    async fn cleanup_shape_database(name: &str, pool: PgPool, admin: PgPool) {
        pool.close().await;
        crate::drop_database(&admin, name).await.unwrap();
        admin.close().await;
    }

    async fn bounded_plans(
        pool: &PgPool,
        analyze_shipped: bool,
        include_old: bool,
    ) -> Result<(serde_json::Value, Option<serde_json::Value>), StoreError> {
        let mut transaction = pool.begin().await?;
        sqlx::query("SET LOCAL statement_timeout = '30s'")
            .execute(&mut *transaction)
            .await?;
        sqlx::query("SET LOCAL max_parallel_workers_per_gather = 0")
            .execute(&mut *transaction)
            .await?;
        configure_search_transaction(&mut transaction).await?;
        let shipped =
            explain_search(&mut transaction, SEARCH_ELEMENTS_SQL, analyze_shipped).await?;
        let old = if include_old {
            Some(explain_search(&mut transaction, &old_search_elements_sql(), false).await?)
        } else {
            None
        };
        transaction.commit().await?;
        Ok((shipped, old))
    }

    async fn seeded_shape_database() -> (String, PgPool, PgPool) {
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
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM embeddings_1024")
                .fetch_one(&pool)
                .await
                .unwrap(),
            20_000
        );
        (name, pool, admin)
    }

    fn assert_static_shipped_shape(shipped: &serde_json::Value) {
        let driver = vector_driver(shipped)
            .unwrap_or_else(|| panic!("shipped plan has no HNSW vector driver: {shipped:#}"));
        assert!(
            driver.ordered_by_distance,
            "HNSW driver has no <=> order: {driver:?}\n{shipped:#}"
        );
        let (estimated_rows, _) = candidate_vector_rows(shipped)
            .unwrap_or_else(|| panic!("candidate_vectors CTE has no bounded Limit: {shipped:#}"));
        assert!(
            estimated_rows <= 160.0,
            "candidate_vectors estimates {estimated_rows} rows, above candidate_limit 160: {shipped:#}"
        );
        assert_eq!(
            driver.child_count, 0,
            "admission was pushed beneath the vector scan: {driver:?}\n{shipped:#}"
        );
        assert!(
            !materializes_elements(shipped),
            "shipped plan materializes an elements scan: {shipped:#}"
        );
        assert!(
            !has_correlated_smart_subplan(shipped),
            "shipped plan retains a correlated smart_content SubPlan: {shipped:#}"
        );
        assert_eq!(
            candidate_vector_target_count(shipped),
            Some(4),
            "candidate_vectors must carry source_hash, source_kind, chunk_no, distance only: {shipped:#}"
        );
        assert!(
            shipped[0].get("JIT").is_none(),
            "JIT must not trigger for the shipped query: {shipped:#}"
        );
    }

    #[tokio::test]
    async fn shipped_search_transaction_disables_jit_locally() {
        let (name, pool, admin) = shape_database().await;
        let verdict: Result<String, StoreError> = async {
            let mut transaction = pool.begin().await?;
            sqlx::query("SET LOCAL jit = on")
                .execute(&mut *transaction)
                .await?;
            configure_search_transaction(&mut transaction).await?;
            let jit: String = sqlx::query_scalar("SHOW jit")
                .fetch_one(&mut *transaction)
                .await?;
            transaction.rollback().await?;
            Ok(jit)
        }
        .await;
        cleanup_shape_database(&name, pool, admin).await;
        assert_eq!(verdict.unwrap(), "off");
    }

    /// A cached generic plan replaced the HNSW arm with a scan of every
    /// vector once a pooled connection had run the search five times.
    #[tokio::test]
    async fn shipped_search_transaction_plans_every_search_for_its_values() {
        let (name, pool, admin) = shape_database().await;
        let verdict: Result<String, StoreError> = async {
            let mut transaction = pool.begin().await?;
            configure_search_transaction(&mut transaction).await?;
            let mode: String = sqlx::query_scalar("SHOW plan_cache_mode")
                .fetch_one(&mut *transaction)
                .await?;
            transaction.rollback().await?;
            Ok(mode)
        }
        .await;
        cleanup_shape_database(&name, pool, admin).await;
        assert_eq!(verdict.unwrap(), "force_custom_plan");
    }

    #[tokio::test]
    async fn search_plan_shape_static_keeps_hnsw_first_and_rejects_the_old_query() {
        let (name, pool, admin) = seeded_shape_database().await;
        let plans = bounded_plans(&pool, false, true).await;
        cleanup_shape_database(&name, pool, admin).await;
        let (shipped, old) =
            plans.unwrap_or_else(|error| panic!("bounded non-ANALYZE plan check failed: {error}"));
        let old = old.expect("old mutation plan requested");

        assert_static_shipped_shape(&shipped);
        assert!(
            has_correlated_smart_subplan(&old),
            "mutation failed: old admission has no correlated smart_content SubPlan: {old:#}"
        );
        assert!(
            materializes_elements(&old),
            "mutation failed: old admission has no Materialize over elements scan: {old:#}"
        );
    }

    #[tokio::test]
    async fn search_plan_shape_analyze_bounds_admission_work() {
        let (name, pool, admin) = seeded_shape_database().await;
        let plans = bounded_plans(&pool, true, false).await;
        cleanup_shape_database(&name, pool, admin).await;
        let (shipped, _) = plans.unwrap_or_else(|error| {
            panic!("bounded shipped-query ANALYZE failed; timeout is a red verdict: {error}")
        });

        assert_static_shipped_shape(&shipped);
        let driver = vector_driver(&shipped).expect("static shape already checked the driver");
        assert!(
            driver.actual_rows.is_some_and(|rows| rows <= 160.0),
            "HNSW driver returned {:?} rows, above candidate_limit 160: {shipped:#}",
            driver.actual_rows
        );
        let (_, actual_rows) = candidate_vector_rows(&shipped)
            .expect("static shape already checked the candidate page");
        assert!(
            actual_rows.is_some_and(|rows| rows <= 160.0),
            "candidate_vectors returned {actual_rows:?} rows, above candidate_limit 160: {shipped:#}"
        );
        let shipped_loops = smart_content_max_loops(&shipped)
            .unwrap_or_else(|| panic!("shipped plan has no smart_content node: {shipped:#}"));
        assert!(
            shipped_loops <= 160.0,
            "smart_content loops {shipped_loops} exceed candidate_limit 160: {shipped:#}"
        );
        let execution_ms = shipped[0]
            .get("Execution Time")
            .and_then(serde_json::Value::as_f64)
            .expect("ANALYZE reports execution time");
        let shared_hits = shipped[0]["Plan"]
            .get("Shared Hit Blocks")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        println!(
            "search_plan_after: execution_ms={execution_ms} shared_hits={shared_hits} hnsw_rows={} candidate_rows={} smart_content_max_loops={shipped_loops}",
            driver.actual_rows.expect("ANALYZE reports HNSW rows"),
            actual_rows.expect("ANALYZE reports candidate rows"),
        );
    }
}
