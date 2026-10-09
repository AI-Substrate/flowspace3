//! Indexed exact-text retrieval over stored element names and source.
//!
//! `pg_trgm` is the deliberate fit: the ruled contract is verbatim substring
//! recall for phrases, snake_case identifiers, and punctuated error codes.
//! A `tsvector` candidate was smaller and slightly faster for one phrase, but
//! tokenizes punctuation and therefore cannot prove verbatim identity. The
//! migration's one combined trigram index keeps that identity check indexed.
//!
//! # Terms, not only the phrase (2026-10-09)
//!
//! A multi-word query used to match only where its words sat together, so
//! "haiku tps" missed every turn that said both apart, and every hit scored a
//! flat 1.0 — a turn that merely quoted the question tied with the answer, and
//! won on recency. Now every term must occur (each its own `LIKE`, so the same
//! trigram index ANDs them in one bitmap scan) and the score is graded.
//!
//! Full-text search was measured and set aside for this: on 1.21M elements a
//! tsvector index is a further 1–2 GB, and ranking needs either a stored
//! column (rewriting a 4.4 GB table under lock) or `to_tsvector` per candidate
//! at query time. Term-AND on the existing index cost ~1.4x the phrase scan it
//! replaces (e.g. 510 → 718 ms) and finds scattered terms the phrase missed.

use fs3_core::Element;
use sqlx::Row;

use crate::{PgPool, SearchFilters, StoreError};

/// The most query terms matched. Longest first, so the most selective survive.
const MAX_TERMS: usize = 8;

/// A small bonus for dense, short matches: under 0.04, so it orders hits
/// WITHIN a score band and can never lift one into the band above.
const DENSITY_SQL: &str = "(0.04 / (1.0 + ln(1.0 + length(el.raw_text) / 2000.0)))";

/// Why an exact lexical hit outranked another exact lexical hit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LexicalMatch {
    /// The query occurs in the declaration's own name.
    Name,
    /// The query occurs, as a phrase, only in the element source.
    Text,
    /// Every term of the query occurs, but not as one phrase.
    Terms,
}

impl LexicalMatch {
    /// Stable wire spelling used by the daemon's `match_field`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Text => "text",
            Self::Terms => "terms",
        }
    }
}

/// One exact lexical hit, resolved to the content and live location it names.
#[derive(Clone, Debug, PartialEq)]
pub struct LexicalHit {
    pub element: Element,
    pub blob_sha: String,
    pub parser_version: String,
    pub identity: Option<String>,
    pub root_path: Option<String>,
    pub path: Option<String>,
    pub matched: LexicalMatch,
    /// How well it matches, in `(0, 1]`: see [`search_lexical_page`].
    pub score: f64,
}

/// Find elements containing every term of the query, best matches first.
///
/// Every ownership, path, kind, and ddoc predicate mirrors `search_elements`.
/// The `LIKE` expression is byte-for-byte the expression indexed by migration
/// 0018; changing either side independently can silently restore a table scan.
///
/// # Errors
/// [`StoreError::Query`] on database failure; [`StoreError::Corrupt`] when a
/// stored element kind cannot be decoded.
pub async fn search_lexical(
    pool: &PgPool,
    query: &str,
    filters: &SearchFilters,
) -> Result<Vec<LexicalHit>, StoreError> {
    Ok(search_lexical_page(pool, query, filters).await?.hits)
}

/// Exact lexical hits plus what the turn-class filter hid from them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LexicalPage {
    /// The visible hits, best first.
    pub hits: Vec<LexicalHit>,
    /// Matches left out because their turn class is hidden, counted within
    /// the window the page was cut from.
    pub hidden_turns: u64,
}

/// [`search_lexical`], also reporting how many matches the turn-class filter
/// hid.
///
/// # The score
///
/// | match | score |
/// |---|---|
/// | code or document element whose name equals the query, or that contains the whole phrase | 1.0 |
/// | the name contains every term | 0.90 + density |
/// | a conversation turn containing the whole phrase | 0.80 + density |
/// | every term, scattered | 0.65 + density |
///
/// Verbatim identity in CODE stays a pinned 1.0, the ruled lexical anchor for
/// identifiers and error codes. Conversation turns are graded instead, because
/// that is where a quote of the question lives, and a quote must not tie
/// with the answer.
///
/// # Errors
/// As [`search_lexical`].
pub async fn search_lexical_page(
    pool: &PgPool,
    query: &str,
    filters: &SearchFilters,
) -> Result<LexicalPage, StoreError> {
    // pg_trgm cannot derive an index key below three characters. Skipping the
    // lexical leg keeps a tiny query from turning into a whole-table scan;
    // the semantic leg still answers it.
    if query.chars().take(3).count() < 3 {
        return Ok(LexicalPage::default());
    }
    let pattern = contains_pattern(query);
    let terms = query_terms(query);

    let text = "lower(el.name || E'\\n' || el.raw_text)";
    let phrase = format!("{text} LIKE $1 ESCAPE '\\'");
    let each = |column: &str| -> String {
        (0..terms.len())
            .map(|i| format!("{column} LIKE ${} ESCAPE '\\'", 13 + i))
            .collect::<Vec<_>>()
            .join(" AND ")
    };
    // Every term must occur. With no term long enough to index, the phrase
    // alone decides, exactly as before.
    let matches = if terms.is_empty() {
        phrase.clone()
    } else {
        each(text)
    };
    let name_has_every_term = if terms.is_empty() {
        "FALSE".to_string()
    } else {
        each("lower(el.name)")
    };

    // Bind map: $1 phrase pattern, $2 limit, $3 repo, $4 path, $5 kinds,
    // $6 worktree, $7 id_kinds, $8 gate_open, $9 ddoc_schema,
    // $10 conversation, $11 hidden turn classes, $12 the query lowercased,
    // $13.. one pattern per term.
    let statement = format!(
        r#"WITH scoped AS (
             SELECT el.id, el.blob_sha, el.parser_version, el.kind, el.subkind,
                    el.name, el.address, el.span_start, el.span_end,
                    el.sibling_order, el.raw_text, el.ddoc,
                    lower(el.name) LIKE $1 ESCAPE '\' AS name_match,
                    {phrase} AS phrase_match,
                    COALESCE(el.turn_class = ANY($11::text[]), FALSE) AS hidden,
                    (CASE
                       WHEN el.kind <> 'turn' AND (lower(el.name) = $12 OR {phrase})
                         THEN 1.0
                       WHEN {name_has_every_term} THEN 0.90 + {DENSITY_SQL}
                       WHEN {phrase} THEN 0.80 + {DENSITY_SQL}
                       ELSE 0.65 + {DENSITY_SQL}
                     END)::float8 AS score
               FROM elements el
              WHERE {matches}
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
                -- Match semantic indexing: a file covered by child elements
                -- is a container, not a duplicate answer for every child hit.
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
                             AND ($6::text IS NULL OR c.worktree IS NULL OR c.worktree = $6)))
              ORDER BY score DESC, name_match DESC, length(el.raw_text), el.id
              -- A window wider than the page, so hidden echoes cannot starve
              -- it and the disclosure has something to count.
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
         -- Driven from the count, so a page whose every match was hidden
         -- still reports how many: one row with a NULL candidate.
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
                   length(candidate.raw_text), candidate.id"#
    );

    let bound = sqlx::query(&statement)
        .bind(pattern)
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
        .bind(query.trim().to_lowercase());
    let rows = terms
        .iter()
        .fold(bound, |bound, term| bound.bind(contains_pattern(term)))
        .fetch_all(pool)
        .await?;

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
    Ok(LexicalPage { hits, hidden_turns })
}

/// The query's terms worth matching: lowercased, stripped of surrounding
/// punctuation, at least three characters (the trigram index's floor),
/// de-duplicated, longest first, at most [`MAX_TERMS`].
fn query_terms(query: &str) -> Vec<String> {
    let mut terms: Vec<String> = Vec::new();
    for word in query.split_whitespace() {
        let term = word
            .trim_matches(|c: char| !c.is_alphanumeric() && c != '_')
            .to_lowercase();
        if term.chars().count() >= 3 && !terms.contains(&term) {
            terms.push(term);
        }
    }
    terms.sort_by_key(|term| std::cmp::Reverse(term.chars().count()));
    terms.truncate(MAX_TERMS);
    terms
}

fn contains_pattern(query: &str) -> String {
    let mut pattern = String::with_capacity(query.len() + 2);
    pattern.push('%');
    for ch in query.chars().flat_map(char::to_lowercase) {
        if matches!(ch, '\\' | '%' | '_') {
            pattern.push('\\');
        }
        pattern.push(ch);
    }
    pattern.push('%');
    pattern
}

#[cfg(test)]
mod tests {
    use super::{contains_pattern, query_terms};

    #[test]
    fn substring_patterns_keep_like_metacharacters_literal() {
        assert_eq!(contains_pattern("search_elements"), "%search\\_elements%");
        assert_eq!(contains_pattern("100% \\ ready"), "%100\\% \\\\ ready%");
        assert_eq!(contains_pattern("MiXeD"), "%mixed%");
    }

    #[test]
    fn terms_are_lowercased_trimmed_deduplicated_and_longest_first() {
        assert_eq!(query_terms("Haiku TPS"), ["haiku", "tps"]);
        assert_eq!(
            query_terms("which repo + agent compared \"LLM\" pricing, pricing?"),
            ["compared", "pricing", "which", "agent", "repo", "llm"]
        );
        assert_eq!(
            query_terms("embed_tokens_per_minute"),
            ["embed_tokens_per_minute"]
        );
        assert!(
            query_terms("a b c").is_empty(),
            "nothing indexable: the phrase decides"
        );
        assert_eq!(
            query_terms("one two three four five six seven eight nine").len(),
            8
        );
    }
}
