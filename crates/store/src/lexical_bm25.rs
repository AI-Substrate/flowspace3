//! The keyword leg on a pg_search (ParadeDB) BM25 index.
//!
//! Same contract and the same score as [`crate::lexical`]: every term must
//! occur verbatim (today's `LIKE`, case-folded), and hits are graded by the
//! same tiers. What changes is how candidates are FOUND. The trigram leg
//! scores every match it can find, rechecking each one's whole text, which
//! costs seconds for an identifier and up to 13 s for a common word on 1.25M
//! elements. This leg asks the inverted index for the best few windows
//! and verifies only those: p95 0.5 s in every category on the same corpus
//! (`scratch/search-speed/H2-keyword-index-findings.md`, 2026-10-14).
//!
//! # Candidates
//!
//! The UNION of three windows, each the top `limit * 10` by BM25 within the
//! caller's scope, so a tier-defining hit cannot fall out of one shared
//! window behind thousands of common-word matches:
//!
//! | window | finds | feeds tier |
//! |---|---|---|
//! | terms | every term in name or text | 0.65 |
//! | name | every term in the declaration's name | 0.90, and 1.0 for an exact name |
//! | phrase | the whole query, in order | 1.0 in code, 0.80 in a turn |
//!
//! # Matching at token starts
//!
//! Each term matches as its exact phrase OR as a prefix phrase, so `job`
//! finds "jobs", `debounce` finds "debounced", and `lexical_page` finds
//! `search_lexical_page`. The verbatim `LIKE` then decides exactly as the
//! trigram leg does. The one difference is a substring that starts INSIDE a
//! token (`ounded` in `bounded`): that is no longer a candidate. Measured,
//! 4 of 6,088 `debounce` matches. A query with no token long enough to mean
//! anything (`E'\n'`) is the trigram leg's job: see [`bm25_can_answer`].
//!
//! # Case
//!
//! The `source_code` tokenizer splits camelCase only while the case is there
//! (`SearchFilters` -> `search`, `filters`; `searchfilters` stays one token).
//! Terms are therefore passed in the caller's case, ORed with the lowercase
//! form so a run-together spelling in the text still matches.

use sqlx::{PgConnection, Row};

use crate::lexical::{
    DENSITY_SQL, LexicalHit, LexicalMatch, LexicalPage, contains_pattern, query_terms,
};
use crate::{PgPool, SearchFilters, StoreError};

pub use fs3_core::LexicalBackend;

/// The index this leg reads. One ParadeDB index per table is the rule, so it
/// carries every field the leg searches or filters on.
pub const BM25_INDEX: &str = "elements_bm25_idx";

/// How the index is built. The field expressions are byte for byte the ones
/// the leg's statement searches; changing either side alone silently stops
/// the index from being used.
pub const BM25_INDEX_DDL: &str = r"CREATE INDEX CONCURRENTLY IF NOT EXISTS elements_bm25_idx
    ON elements USING bm25 (
        id,
        ((name || E'\n' || raw_text)::pdb.source_code('alias=body')),
        (name::pdb.source_code),
        (kind::pdb.literal),
        (turn_class::pdb.literal)
    )";

/// How many dictionary terms one prefix may expand to. Short prefixes of
/// common words expand furthest; past this, rarer completions are dropped.
const PREFIX_EXPANSIONS: u32 = 256;

const BODY: &str = "(el.name || E'\\n' || el.raw_text)";

/// Whether this leg can answer the query, or the trigram leg must.
///
/// BM25 finds candidates by token, so it needs at least one token of three
/// or more characters, split the way `source_code` splits: at punctuation,
/// at camelCase humps and at letter/digit boundaries. `E'\n'` (`e`, `n`) or
/// `a.b` has none; only a substring scan can find those verbatim.
#[must_use]
pub fn bm25_can_answer(query: &str) -> bool {
    source_code_tokens(query)
        .iter()
        .any(|token| token.chars().count() >= 3)
}

/// An approximation of pg_search's `source_code` tokenizer, enough to route.
fn source_code_tokens(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    for word in text.split(|c: char| !c.is_alphanumeric()) {
        let chars: Vec<char> = word.chars().collect();
        let mut start = 0;
        for i in 1..chars.len() {
            let (prev, cur) = (chars[i - 1], chars[i]);
            let next_lower = chars.get(i + 1).is_some_and(|c| c.is_lowercase());
            let boundary = (prev.is_lowercase() && cur.is_uppercase())
                || (prev.is_alphabetic() != cur.is_alphabetic())
                || (prev.is_uppercase() && cur.is_uppercase() && next_lower);
            if boundary {
                tokens.push(chars[start..i].iter().collect::<String>().to_lowercase());
                start = i;
            }
        }
        if start < chars.len() {
            tokens.push(chars[start..].iter().collect::<String>().to_lowercase());
        }
    }
    tokens
}

/// Each lexical term with the spelling the caller used for it.
fn cased_terms(query: &str) -> Vec<(String, String)> {
    query_terms(query)
        .into_iter()
        .map(|lower| {
            let cased = query
                .split_whitespace()
                .map(|word| word.trim_matches(|c: char| !c.is_alphanumeric() && c != '_'))
                .find(|word| word.to_lowercase() == lower)
                .map_or_else(|| lower.clone(), str::to_string);
            (lower, cased)
        })
        .collect()
}

/// `field` holds the text bound at `$param`, exactly or as a prefix phrase.
fn token_match(field: &str, param: usize) -> String {
    format!(
        "{field} ### ${param}::text OR {field} @@@ pdb.phrase_prefix((${param}::text::pdb.source_code)::text[], {PREFIX_EXPANSIONS})"
    )
}

/// Same predicates as `search_lexical_page`, applied INSIDE each window so
/// the window is cut within the caller's scope, not across every repository.
const FILTERS_SQL: &str = r#"
                AND ($5::text[] IS NULL OR el.kind = ANY($5))
                AND ($7::text[] IS NULL OR el.ddoc->>'id_kind' = ANY($7))
                AND ($8::boolean IS NULL
                     OR (CASE
                           WHEN jsonb_typeof(el.ddoc->'derived_state') = 'object'
                           THEN (el.ddoc->'derived_state'->>'complete')::boolean
                           ELSE (el.ddoc->>'gate_terminal')::boolean
                         END IS NOT NULL
                         AND CASE
                           WHEN jsonb_typeof(el.ddoc->'derived_state') = 'object'
                           THEN (el.ddoc->'derived_state'->>'complete')::boolean
                           ELSE (el.ddoc->>'gate_terminal')::boolean
                         END = NOT $8))
                AND ($9::text IS NULL OR el.ddoc->>'schema' = $9)
                AND ($10::text IS NULL
                     OR strpos(el.address, 'conv:' || $10 || '#t') = 1)
                AND (el.kind <> 'file' OR NOT EXISTS (
                     SELECT 1 FROM elements child WHERE child.parent_id = el.id))
                AND ($3::text IS NULL AND $4::text IS NULL AND $6::text IS NULL
                     OR EXISTS (
                          SELECT 1
                            FROM worktree_files f
                            JOIN worktrees w ON w.id = f.worktree_id
                            JOIN repos r     ON r.id = w.repo_id
                           WHERE f.blob_sha = el.blob_sha
                             AND ($3::text IS NULL OR r.identity = $3)
                             AND ($4::text IS NULL OR f.path LIKE $4)
                             AND ($6::text IS NULL OR w.root_path = $6))
                     OR EXISTS (
                          SELECT 1
                            FROM turns t
                            JOIN conversations c ON c.guid = t.conversation_id
                           WHERE t.blob_sha = el.blob_sha
                             AND ($3::text IS NULL OR c.repo_identity = $3)
                             AND ($4::text IS NULL OR c.worktree LIKE $4)
                             AND ($6::text IS NULL OR c.worktree IS NULL OR c.worktree = $6)))"#;

/// The statement for `terms.len()` terms, and which of them need their
/// lowercase spelling too. Bind map: $1-$12 as `search_lexical_page`, $13
/// the trimmed query, then per term `i`: $14+3i its `LIKE` pattern, $15+3i
/// its cased spelling, $16+3i its lowercase spelling.
fn statement(terms: &[(String, String)], phrase_window: bool) -> String {
    let pattern = |i: usize| 14 + 3 * i;
    let every = |field: &str| -> String {
        terms
            .iter()
            .enumerate()
            .map(|(i, (lower, cased))| {
                let mut clause = token_match(field, pattern(i) + 1);
                if lower != cased {
                    clause = format!("{clause} OR {}", token_match(field, pattern(i) + 2));
                }
                format!("({clause})")
            })
            .collect::<Vec<_>>()
            .join(" AND ")
    };
    let window = |matches: String| {
        format!(
            "(SELECT el.id FROM elements el
               WHERE {matches}{FILTERS_SQL}
               ORDER BY pdb.score(el.id) DESC
               LIMIT $2 * 10)"
        )
    };
    let mut windows = Vec::new();
    if !terms.is_empty() {
        windows.push(window(every(BODY)));
        windows.push(window(every("el.name")));
    }
    if phrase_window || terms.is_empty() {
        windows.push(window(format!("({})", token_match(BODY, 13))));
    }
    let verbatim = if terms.is_empty() {
        "lc.t LIKE $1 ESCAPE '\\'".to_string()
    } else {
        (0..terms.len())
            .map(|i| format!("lc.t LIKE ${} ESCAPE '\\'", pattern(i)))
            .collect::<Vec<_>>()
            .join(" AND ")
    };
    let name_has_every_term = if terms.is_empty() {
        "FALSE".to_string()
    } else {
        (0..terms.len())
            .map(|i| format!("lc.n LIKE ${} ESCAPE '\\'", pattern(i)))
            .collect::<Vec<_>>()
            .join(" AND ")
    };
    let phrase = "lc.t LIKE $1 ESCAPE '\\'";
    format!(
        r#"WITH hits AS MATERIALIZED (
             {windows}
         ),
         scoped AS (
             SELECT el.id, el.blob_sha, el.parser_version, el.kind, el.subkind,
                    el.name, el.address, el.span_start, el.span_end,
                    el.sibling_order, el.raw_text, el.ddoc,
                    lc.n LIKE $1 ESCAPE '\' AS name_match,
                    {phrase} AS phrase_match,
                    COALESCE(el.turn_class = ANY($11::text[]), FALSE) AS hidden,
                    (CASE
                       WHEN el.kind <> 'turn' AND (lc.n = $12 OR {phrase})
                         THEN 1.0
                       WHEN {name_has_every_term} THEN 0.90 + {DENSITY_SQL}
                       WHEN {phrase} THEN 0.80 + {DENSITY_SQL}
                       ELSE 0.65 + {DENSITY_SQL}
                     END)::float8 AS score
               FROM hits
               JOIN elements el ON el.id = hits.id
              CROSS JOIN LATERAL (
                   SELECT lower(el.name || E'\n' || el.raw_text) AS t,
                          lower(el.name) AS n
                   OFFSET 0
              ) lc
              WHERE {verbatim}
              ORDER BY score DESC, name_match DESC, length(el.raw_text), el.id
              LIMIT $2 * 10
         ),
         candidates AS (
             SELECT * FROM scoped WHERE NOT hidden
              ORDER BY score DESC, name_match DESC, length(raw_text), id
              LIMIT $2
         ),
         hidden_meta AS (
             SELECT count(*)::bigint AS hidden_count FROM scoped WHERE hidden
         )
         SELECT candidate.*, hidden.hidden_count,
                COALESCE(live.identity, anchored.identity) AS identity,
                COALESCE(live.root_path, anchored.root_path) AS root_path,
                live.path
           FROM hidden_meta hidden
           LEFT JOIN candidates candidate ON TRUE
           LEFT JOIN LATERAL (
                SELECT r.identity, w.root_path, f.path
                  FROM worktree_files f
                  JOIN worktrees w ON w.id = f.worktree_id
                  JOIN repos r     ON r.id = w.repo_id
                 WHERE f.blob_sha = candidate.blob_sha
                   AND ($3::text IS NULL OR r.identity = $3)
                   AND ($4::text IS NULL OR f.path LIKE $4)
                   AND ($6::text IS NULL OR w.root_path = $6)
                 ORDER BY r.identity, w.root_path, f.path
                 LIMIT 1
           ) live ON TRUE
           LEFT JOIN LATERAL (
                SELECT c.repo_identity AS identity, c.worktree AS root_path
                  FROM turns t
                  JOIN conversations c ON c.guid = t.conversation_id
                 WHERE t.blob_sha = candidate.blob_sha
                   AND ($3::text IS NULL OR c.repo_identity = $3)
                   AND ($4::text IS NULL OR c.worktree LIKE $4)
                   AND ($6::text IS NULL OR c.worktree IS NULL OR c.worktree = $6)
                   AND ($10::text IS NULL OR c.guid = $10::uuid)
                 ORDER BY c.repo_identity, c.worktree
                 LIMIT 1
           ) anchored ON TRUE
          ORDER BY candidate.score DESC, candidate.name_match DESC,
                   length(candidate.raw_text), candidate.id"#,
        windows = windows.join("\n             UNION\n             ")
    )
}

/// [`crate::search_lexical_page`] answered from the BM25 index: the same
/// hits, scores and hidden-turn disclosure, found by index instead of scan.
///
/// Callers normally reach this through [`crate::search_lexical_pages`] with
/// [`SearchFilters::lexical_backend`] set to BM25; asked directly, it
/// answers what the query's tokens can reach.
///
/// # Errors
/// [`StoreError::Query`] on database failure, including a database without
/// the `pg_search` extension or the [`BM25_INDEX`]; [`StoreError::Corrupt`]
/// when a stored element kind cannot be decoded.
pub async fn search_bm25_page(
    pool: &PgPool,
    query: &str,
    filters: &SearchFilters,
) -> Result<LexicalPage, StoreError> {
    Ok(search_bm25_pages(pool, query, filters, false).await?.0)
}

/// [`crate::search_lexical_pages`] on the BM25 index: the scoped page and,
/// when `wide`, the same query across every repository.
///
/// Each page is one index lookup of a few milliseconds to a few hundred, so
/// the two run as two statements in one transaction, under the caller's
/// budget, rather than one statement that shares a scan as the trigram leg
/// must.
///
/// # Errors
/// As [`search_bm25_page`].
pub async fn search_bm25_pages(
    pool: &PgPool,
    query: &str,
    filters: &SearchFilters,
    wide: bool,
) -> Result<(LexicalPage, Option<LexicalPage>), StoreError> {
    let query = query.trim();
    if query.chars().take(3).count() < 3 {
        return Ok((LexicalPage::default(), wide.then(LexicalPage::default)));
    }
    let acquire_started = std::time::Instant::now();
    let mut tx = pool.begin().await?;
    crate::embeddings::apply_budget(&mut tx, filters.budget).await?;
    let acquire_ms = crate::elapsed_ms(acquire_started);

    let mut scoped = page_on(&mut tx, query, filters).await?;
    scoped.acquire_ms = acquire_ms;
    let other = if wide {
        let unscoped = SearchFilters {
            repo: None,
            worktree: None,
            ..filters.clone()
        };
        let mut page = page_on(&mut tx, query, &unscoped).await?;
        page.acquire_ms = acquire_ms;
        Some(page)
    } else {
        None
    };
    tx.commit().await?;
    Ok((scoped, other))
}

/// One page, on a connection the caller holds.
async fn page_on(
    connection: &mut PgConnection,
    query: &str,
    filters: &SearchFilters,
) -> Result<LexicalPage, StoreError> {
    let terms = cased_terms(query);
    let phrase_window = terms.len() != 1 || terms[0].0 != query.to_lowercase();
    let statement = statement(&terms, phrase_window);

    let mut bound = sqlx::query(&statement)
        .bind(contains_pattern(query))
        .bind(filters.limit)
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
        .bind(filters.hidden_turn_class_names())
        .bind(query.to_lowercase())
        .bind(query.to_string());
    for (lower, cased) in &terms {
        bound = bound
            .bind(contains_pattern(lower))
            .bind(cased.clone())
            .bind(lower.clone());
    }
    let query_started = std::time::Instant::now();
    let rows = bound.fetch_all(&mut *connection).await?;
    let query_ms = crate::elapsed_ms(query_started);

    let hidden_turns = rows
        .first()
        .map(|row| row.try_get::<i64, _>("hidden_count"))
        .transpose()?
        .map_or(0, |count| u64::try_from(count).unwrap_or(0));
    let hits = rows
        .iter()
        .filter(|row| matches!(row.try_get::<Option<i64>, _>("id"), Ok(Some(_))))
        .map(|row| {
            let matched = if row.try_get("name_match")? {
                LexicalMatch::Name
            } else if row.try_get("phrase_match")? {
                LexicalMatch::Text
            } else {
                LexicalMatch::Terms
            };
            Ok(LexicalHit {
                element: crate::elements::element_from_row(row)?,
                blob_sha: row.try_get("blob_sha")?,
                parser_version: row.try_get("parser_version")?,
                identity: row.try_get("identity")?,
                root_path: row.try_get("root_path")?,
                path: row.try_get("path")?,
                matched,
                score: row.try_get("score")?,
            })
        })
        .collect::<Result<Vec<_>, StoreError>>()?;
    Ok(LexicalPage {
        hits,
        hidden_turns,
        acquire_ms: 0,
        query_ms,
    })
}

/// Whether this database can serve [`search_bm25_page`]: pg_search is
/// installed and [`BM25_INDEX`] is built and valid.
///
/// # Errors
/// [`StoreError::Query`] on database failure.
pub async fn bm25_index_ready(pool: &PgPool) -> Result<bool, StoreError> {
    let ready: Option<bool> = sqlx::query_scalar(
        "SELECT i.indisvalid AND i.indisready
           FROM pg_index i
           JOIN pg_class c ON c.oid = i.indexrelid
          WHERE c.relname = $1",
    )
    .bind(BM25_INDEX)
    .fetch_optional(pool)
    .await?;
    Ok(ready.unwrap_or(false))
}

/// Where this database stands on the BM25 keyword index, from "this
/// Postgres cannot have one" to "ready".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bm25IndexState {
    /// The server has no `pg_search`: it runs an image without it.
    NoExtension,
    /// `pg_search` is available, and [`BM25_INDEX`] does not exist.
    Missing,
    /// [`BM25_INDEX`] exists but is not valid: a `CONCURRENTLY` build that
    /// failed or was interrupted leaves one behind.
    Invalid,
    /// Built and valid: [`search_bm25_page`] can answer.
    Ready,
}

/// Reads [`Bm25IndexState`] without changing anything.
///
/// # Errors
/// [`StoreError::Query`] on database failure.
pub async fn bm25_index_state(pool: &PgPool) -> Result<Bm25IndexState, StoreError> {
    let available: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM pg_available_extensions WHERE name = 'pg_search')",
    )
    .fetch_one(pool)
    .await?;
    if !available {
        return Ok(Bm25IndexState::NoExtension);
    }
    let valid: Option<bool> = sqlx::query_scalar(
        "SELECT i.indisvalid AND i.indisready
           FROM pg_index i
           JOIN pg_class c ON c.oid = i.indexrelid
          WHERE c.relname = $1",
    )
    .bind(BM25_INDEX)
    .fetch_optional(pool)
    .await?;
    Ok(match valid {
        None => Bm25IndexState::Missing,
        Some(false) => Bm25IndexState::Invalid,
        Some(true) => Bm25IndexState::Ready,
    })
}

/// Creates `pg_search` and builds [`BM25_INDEX`] beside the trigram index,
/// `CONCURRENTLY`, so writes and trigram searches carry on while it builds.
/// An invalid index left by an earlier failed build is dropped first, since
/// `IF NOT EXISTS` would otherwise keep it. On a valid index it changes
/// nothing. Returns the index's size in bytes.
///
/// Not a migration: migrations run at boot inside a transaction, which
/// `CONCURRENTLY` cannot, and must not fail on a Postgres without pg_search.
///
/// # Errors
/// [`StoreError::Query`] when the extension or the build fails, including on
/// a Postgres without `pg_search` (check [`bm25_index_state`] first).
pub async fn build_bm25_index(pool: &PgPool) -> Result<i64, StoreError> {
    sqlx::query("CREATE EXTENSION IF NOT EXISTS pg_search")
        .execute(pool)
        .await?;
    if bm25_index_state(pool).await? == Bm25IndexState::Invalid {
        sqlx::query(&format!("DROP INDEX CONCURRENTLY IF EXISTS {BM25_INDEX}"))
            .execute(pool)
            .await?;
    }
    sqlx::query(BM25_INDEX_DDL).execute(pool).await?;
    let bytes: i64 = sqlx::query_scalar("SELECT pg_relation_size($1::regclass)")
        .bind(BM25_INDEX)
        .fetch_one(pool)
        .await?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::{bm25_can_answer, cased_terms, source_code_tokens, statement};

    #[test]
    fn tokens_split_like_source_code() {
        assert_eq!(
            source_code_tokens("bounded_error(REQUEST_BYTES) searchElements E0425 HTTPServer"),
            [
                "bounded", "error", "request", "bytes", "search", "elements", "e", "0425", "http",
                "server"
            ]
        );
    }

    #[test]
    fn only_token_less_queries_go_to_trigram() {
        for query in [
            "bounded_error",
            "REQUEST_BYTES",
            "SearchFilters",
            "refs/notes/ai",
            "127.0.0.1:5434",
            "--include-retrieval",
            "pg_trgm",
            "E0425",
            "the search",
        ] {
            assert!(bm25_can_answer(query), "{query}");
        }
        for query in ["E'\\n'", "a.b", "x = y", "::"] {
            assert!(!bm25_can_answer(query), "{query}");
        }
    }

    #[test]
    fn terms_keep_the_callers_case_beside_the_lowercase() {
        assert_eq!(
            cased_terms("SearchFilters for MAX_TERMS"),
            [
                ("searchfilters".to_string(), "SearchFilters".to_string()),
                ("max_terms".to_string(), "MAX_TERMS".to_string()),
                ("for".to_string(), "for".to_string()),
            ]
        );
    }

    #[test]
    fn a_lowercase_term_is_matched_once_a_cased_one_twice() {
        let terms = cased_terms("SearchFilters model");
        let sql = statement(&terms, true);
        // SearchFilters: cased ($15) and lowercase ($16); model: cased only ($18).
        assert!(sql.contains("$15::text") && sql.contains("$16::text"));
        assert!(sql.contains("$18::text") && !sql.contains("$19::text"));
        // Three windows: terms, name, and the whole phrase ($13).
        assert_eq!(sql.matches("ORDER BY pdb.score(el.id) DESC").count(), 3);
        assert!(sql.contains("$13::text"));
    }

    #[test]
    fn a_single_term_query_skips_the_phrase_window() {
        let terms = cased_terms("bounded_error");
        assert_eq!(statement(&terms, false).matches("pdb.score").count(), 2);
    }
}
