//! Indexed exact-text retrieval over stored element names and source.
//!
//! `pg_trgm` is the deliberate fit: the ruled contract is verbatim substring
//! recall for phrases, snake_case identifiers, and punctuated error codes.
//! A `tsvector` candidate was smaller and slightly faster for one phrase, but
//! tokenizes punctuation and therefore cannot prove verbatim identity. The
//! migration's one combined trigram index keeps that identity check indexed.

use fs3_core::Element;
use sqlx::Row;

use crate::{PgPool, SearchFilters, StoreError};

/// Why an exact lexical hit outranked another exact lexical hit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LexicalMatch {
    /// The query occurs in the declaration's own name.
    Name,
    /// The query occurs only in the element source.
    Text,
}

impl LexicalMatch {
    /// Stable wire spelling used by the daemon's `match_field`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Text => "text",
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
}

/// Find case-insensitive verbatim substrings, structural names first.
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
    // pg_trgm cannot derive an index key below three characters. Skipping the
    // lexical leg keeps a tiny query from turning into a whole-table scan;
    // the semantic leg still answers it.
    if query.chars().take(3).count() < 3 {
        return Ok(Vec::new());
    }
    let pattern = contains_pattern(query);
    // Bind map: $1 pattern, $2 limit, $3 repo, $4 path, $5 kinds,
    // $6 worktree, $7 id_kinds, $8 gate_open, $9 ddoc_schema,
    // $10 conversation.
    let rows = sqlx::query(
        r#"WITH candidates AS (
             SELECT el.id, el.blob_sha, el.parser_version, el.kind, el.subkind,
                    el.name, el.address, el.span_start, el.span_end,
                    el.sibling_order, el.raw_text, el.ddoc,
                    lower(el.name) LIKE $1 ESCAPE '\' AS name_match
               FROM elements el
              WHERE lower(el.name || E'\n' || el.raw_text) LIKE $1 ESCAPE '\'
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
              ORDER BY name_match DESC, length(el.raw_text), el.id
              LIMIT $2
         )
         SELECT candidate.*,
                COALESCE(live.identity, anchored.identity) AS identity,
                COALESCE(live.root_path, anchored.root_path) AS root_path,
                live.path
           FROM candidates candidate
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
          ORDER BY candidate.name_match DESC, length(candidate.raw_text), candidate.id"#,
    )
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
    .fetch_all(pool)
    .await?;

    rows.iter()
        .map(|row| {
            Ok(LexicalHit {
                element: crate::elements::element_from_row(row)?,
                blob_sha: row.try_get("blob_sha")?,
                parser_version: row.try_get("parser_version")?,
                identity: row.try_get("identity")?,
                root_path: row.try_get("root_path")?,
                path: row.try_get("path")?,
                matched: if row.try_get("name_match")? {
                    LexicalMatch::Name
                } else {
                    LexicalMatch::Text
                },
            })
        })
        .collect()
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
    use super::contains_pattern;

    #[test]
    fn substring_patterns_keep_like_metacharacters_literal() {
        assert_eq!(contains_pattern("search_elements"), "%search\\_elements%");
        assert_eq!(contains_pattern("100% \\ ready"), "%100\\% \\\\ ready%");
        assert_eq!(contains_pattern("MiXeD"), "%mixed%");
    }
}
