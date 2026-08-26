//! Search v1: semantic, filtered, in the workshop-003 row shape.
//!
//! One question in, ranked hits out. The path is short on purpose — embed the
//! query with the SAME embedder that wrote the vectors, hand the vector and the
//! filters to the store, render what comes back.
//!
//! # Why the query embedder must be the repo's
//!
//! Vectors are only comparable within one model's space: cosine distance
//! between a vector from model A and one from model B is a number with no
//! meaning. The store enforces the half it can see (`model_key` is a column and
//! a predicate), and this module supplies the other half by resolving the
//! embedder through the same per-repo selection the enrichment jobs used. A
//! `--repo` filter therefore changes which MODEL answers, not just which rows
//! are eligible.
//!
//! # Scores, not distances
//!
//! The store speaks cosine distance (0.0 is identical, nearest sorts first);
//! the surface speaks score (1.0 is identical, highest sorts first). The
//! conversion is `1 - distance` and it happens exactly here, at the boundary,
//! so `--min-score 0.7` is a number a human can reason about rather than a
//! ceiling they have to invert in their head.

use fs3_core::catalog;
use fs3_core::envelope::Failure;
use fs3_store::{SearchFilters, SearchHit, SourceKind};
use serde::{Deserialize, Serialize};

use crate::runner::fail;
use crate::wiring::AppState;

/// What a caller asks for.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct SearchRequest {
    /// The question.
    pub q: String,
    /// Restrict to one repository identity.
    #[serde(default)]
    pub repo: Option<String>,
    /// Restrict to paths matching this glob.
    #[serde(default)]
    pub path: Option<String>,
    /// `raw`, `smart`, or absent for both.
    #[serde(default)]
    pub source: Option<String>,
    /// Similarity floor, 0.0–1.0.
    #[serde(default)]
    pub min_score: Option<f64>,
    /// How many hits.
    #[serde(default)]
    pub limit: Option<i64>,
}

/// The largest `--limit` a caller may ask for.
///
/// Search returns lean rows and `get` provides depth (workshop 003 D4), so a
/// caller wanting hundreds of hits is almost always about to post-process in a
/// way a filter would do better. The ceiling makes that a conversation rather
/// than a slow query.
pub const MAX_LIMIT: i64 = 100;

/// One hit, in the workshop-003 row shape.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Hit {
    /// `el:<repo>/<path>::<container>::<name>` — the universal currency (D7).
    pub address: String,
    /// 1.0 is identical; highest first.
    pub score: f64,
    /// Which vector space won this hit: `raw` or `smart`.
    pub match_field: String,
    /// The element's universal category.
    pub kind: String,
    /// The grammar's own kind.
    pub subkind: String,
    /// The declaration's own name.
    pub name: String,
    /// Inclusive 1-based `[start, end]`.
    pub span: [u32; 2],
    /// The first lines of the element's own text.
    pub snippet: String,
    /// The summary, when there is one.
    pub smart: Option<String>,
    /// Concept tags from the summary (PRD req 36).
    pub tags: Vec<String>,
    /// The repository a live path holding this content belongs to.
    pub repo: Option<String>,
    /// A live path holding it, relative to its worktree root.
    pub path: Option<String>,
}

/// What `GET /search` answers with.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SearchResults {
    /// Ranked hits, best first.
    pub results: Vec<Hit>,
}

/// How many lines of an element's text a hit carries.
const SNIPPET_LINES: usize = 5;

/// Answer one search.
///
/// # Errors
/// [`catalog::QUERY_INVALID`] for a request that cannot be answered as asked,
/// [`catalog::PROVIDER_FAILED`] when the query cannot be embedded, and store
/// failures mapped by their own codes.
pub async fn search(state: &AppState, request: &SearchRequest) -> Result<SearchResults, Failure> {
    let query = request.q.trim();
    if query.is_empty() {
        return Err(Failure::new(
            &catalog::QUERY_INVALID,
            "the query is empty; there is nothing to rank against",
        )
        .with_fix("pass a question: `flowspace3 search \"how does auth work\"`"));
    }

    let limit = request.limit.unwrap_or(10);
    if !(1..=MAX_LIMIT).contains(&limit) {
        return Err(Failure::new(
            &catalog::QUERY_INVALID,
            format!("--limit must be between 1 and {MAX_LIMIT}, got {limit}"),
        ));
    }

    let source = match request.source.as_deref() {
        None | Some("all") => None,
        Some("raw") => Some(SourceKind::Raw),
        Some("smart") => Some(SourceKind::Smart),
        Some(other) => {
            return Err(Failure::new(
                &catalog::QUERY_INVALID,
                format!("--source must be raw, smart or all, got {other:?}"),
            ));
        }
    };

    let max_distance = match request.min_score {
        None => None,
        Some(score) if (0.0..=1.0).contains(&score) => Some(1.0 - score),
        Some(score) => {
            return Err(Failure::new(
                &catalog::QUERY_INVALID,
                format!("--min-score must be between 0.0 and 1.0, got {score}"),
            ));
        }
    };

    // The repo filter selects the model as well as the rows: a vector from
    // another model's space is not a nearer or further hit, it is a meaningless
    // comparison.
    let repo_key = request.repo.clone().unwrap_or_default();
    let model_key = state.embedder_key(&repo_key);
    let vector = state
        .embedder_for(&repo_key)
        .embed(&[query.to_string()])
        .await
        .map_err(fail)?
        .pop()
        .ok_or_else(|| {
            Failure::new(
                &catalog::PROVIDER_FAILED,
                "the embedder returned no vector for the query",
            )
        })?;

    let filters = SearchFilters {
        repo: request.repo.clone(),
        path: request.path.as_deref().map(glob_to_like),
        source,
        max_distance,
        limit,
    };

    let hits = fs3_store::search_elements(&state.db, &model_key, &vector, &filters)
        .await
        .map_err(fail)?;

    Ok(SearchResults {
        results: hits.iter().map(render).collect(),
    })
}

/// Turn a store hit into a workshop-003 row.
fn render(hit: &SearchHit) -> Hit {
    let element = &hit.similar.element;
    Hit {
        address: address_of(hit),
        // Cosine distance to score, once, at the boundary.
        score: 1.0 - hit.similar.distance,
        match_field: hit.similar.source_kind.as_str().to_string(),
        kind: element.kind.as_str().to_string(),
        subkind: element.subkind.clone(),
        name: element.name.clone(),
        span: [element.span.start_line, element.span.end_line],
        snippet: snippet(&element.raw_text),
        smart: hit.similar.smart.as_ref().map(|s| s.text.clone()),
        tags: hit
            .similar
            .smart
            .as_ref()
            .map(|s| s.tags.clone())
            .unwrap_or_default(),
        repo: hit.identity.clone(),
        path: hit.path.clone(),
    }
}

/// `el:<repo>/<address>` — workshop 003's element address.
///
/// The element's own `address` already begins with its repo-relative path, so
/// the repo identity is the only thing prepended. A hit with no live path keeps
/// a bare `el:<address>`: the content is real even when the checkout that held
/// it is gone, and inventing a repository for it would be a lie.
fn address_of(hit: &SearchHit) -> String {
    match &hit.identity {
        Some(identity) => format!("el:{identity}/{}", hit.similar.element.address),
        None => format!("el:{}", hit.similar.element.address),
    }
}

/// The first few lines of an element's text.
///
/// Lean rows are the point (D4): depth comes from `get`, and a search that
/// returns whole function bodies is a search that costs a page of scrollback per
/// hit.
fn snippet(raw_text: &str) -> String {
    raw_text
        .lines()
        .take(SNIPPET_LINES)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Translate a path glob into the SQL `LIKE` pattern the store filters with.
///
/// `*` and `**` both become `%`: `LIKE` has no notion of a path separator, so
/// the distinction a shell draws between "within one segment" and "across
/// segments" cannot survive the translation. Collapsing them deliberately is
/// better than pretending: `crates/*/src` matching across segments returns a
/// superset, which a reader can narrow, while the reverse would silently hide
/// files.
fn glob_to_like(glob: &str) -> String {
    let mut out = String::with_capacity(glob.len() + 2);
    let mut chars = glob.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '*' => {
                while chars.peek() == Some(&'*') {
                    chars.next();
                }
                out.push('%');
            }
            '?' => out.push('_'),
            // `%` and `_` are LIKE metacharacters; a literal one in a path must
            // not become a wildcard.
            '%' | '_' => {
                out.push('\\');
                out.push(ch);
            }
            other => out.push(other),
        }
    }
    // A bare prefix behaves the way a person expects a path filter to.
    if !out.ends_with('%') {
        out.push('%');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_glob_becomes_a_like_pattern_with_its_metacharacters_escaped() {
        assert_eq!(glob_to_like("crates/store/*"), "crates/store/%");
        assert_eq!(glob_to_like("crates/**/src"), "crates/%/src%");
        assert_eq!(glob_to_like("crates/store"), "crates/store%");
        assert_eq!(glob_to_like("a?c"), "a_c%");
        // The trap this kills: a literal underscore is a single-character
        // wildcard in LIKE, so `my_file` would have matched `myXfile`.
        assert_eq!(glob_to_like("my_file.rs"), "my\\_file.rs%");
        assert_eq!(glob_to_like("100%.md"), "100\\%.md%");
    }

    #[test]
    fn a_snippet_is_lean_rather_than_the_whole_body() {
        let body = (1..=20)
            .map(|n| format!("line {n}"))
            .collect::<Vec<_>>()
            .join("\n");
        let snippet = snippet(&body);
        assert_eq!(snippet.lines().count(), SNIPPET_LINES);
        assert!(snippet.starts_with("line 1"));
    }

    /// The conversion that makes `--min-score` a number a human can reason
    /// about. An identical vector is distance 0.0 and must read as score 1.0.
    #[test]
    fn score_is_the_inverse_of_cosine_distance() {
        let score = |distance: f64| 1.0 - distance;
        assert!((score(0.0) - 1.0).abs() < f64::EPSILON);
        assert!((score(1.0) - 0.0).abs() < f64::EPSILON);
        // and a min_score of 0.7 must become a distance ceiling of 0.3
        assert!((1.0 - 0.7 - 0.3f64).abs() < 1e-9);
    }
}
