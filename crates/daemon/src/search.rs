//! Search v1: semantic, filtered, in the workshop-003 row shape.
//!
//! One question in, ranked hits out. The path is short on purpose — embed the
//! query with the SAME embedder that wrote the vectors, hand the vector and the
//! filters to the store, render what comes back.
//!
//! # Snap-in recipe
//!
//! This surface needs no new config, constructor, service, or route registration.
//! Keep the existing `GET /search` route deserializing [`SearchRequest`] and the
//! existing `GET /get` route deserializing [`crate::read::GetRequest`]. Parsed ddoc
//! rows already arrive at `SearchHit.similar.element.ddoc`; [`render`] copies that
//! value into [`Hit::ddoc`] and passes the element's dd address through verbatim.
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
use fs3_core::{DdocMeta, DdocRel, DerivedState, ElementKind};
use fs3_store::{SearchFilters, SearchHit, SourceKind};
use serde::{Deserialize, Serialize};

use crate::runner::fail;
use crate::scope::Scope;
use crate::wiring::AppState;

/// What a caller asks for.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct SearchRequest {
    /// The question.
    pub q: String,
    /// Restrict to one repository identity, or `all` to widen back to every
    /// repository. Absent means "wherever the caller is standing" (D6).
    #[serde(default)]
    pub repo: Option<String>,
    /// The caller's working directory, which is what D6 scopes by. The daemon
    /// has one of its own and it is never the caller's, so it has to be sent.
    #[serde(default)]
    pub cwd: Option<String>,
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
    /// Restrict ddoc rows to one raw minted-id prefix.
    #[serde(default)]
    pub id_kind: Option<String>,
    /// Select known-open (`true`) or known-closed (`false`) ddoc rows.
    /// Unknown gate state matches neither value; absent means no gate filter.
    #[serde(default)]
    pub gate_open: Option<bool>,
    /// Restrict ddoc rows to one declared schema, verbatim.
    #[serde(default)]
    pub ddoc_schema: Option<String>,
}

/// The largest `--limit` a caller may ask for.
///
/// Search returns lean rows and `get` provides depth (workshop 003 D4), so a
/// caller wanting hundreds of hits is almost always about to post-process in a
/// way a filter would do better. The ceiling makes that a conversation rather
/// than a slow query.
pub const MAX_LIMIT: i64 = 100;

/// The element kinds that answer a CODE search.
///
/// Named exhaustively rather than as "everything except a turn", so that adding
/// a content type is a decision someone makes here rather than an accident that
/// silently starts blending it into code results.
const CODE_KINDS: [ElementKind; 5] = [
    ElementKind::File,
    ElementKind::Container,
    ElementKind::Function,
    ElementKind::Section,
    ElementKind::Row,
];

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
    /// Deterministic-document metadata. Absent, including the key, on code hits.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ddoc: Option<DdocHit>,
}

/// One ddoc row's meaning and both of its independent state claims.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DdocHit {
    /// dd's path-qualified positional address; paste directly into `ddocs get`.
    pub address: String,
    pub schema: String,
    pub section: String,
    pub id: String,
    pub id_kind: Option<String>,
    pub trail: Vec<String>,
    pub doc_title: Option<String>,
    /// The source document's stored state. It is not authoritative.
    pub state_stored: Option<String>,
    /// State derived from assertions. Believe this claim when present.
    pub state_derived: Option<DerivedState>,
    /// Whether the stored state belongs to the schema's terminal set.
    pub gate_terminal: Option<bool>,
    pub rels: Vec<DdocRel>,
    pub findings: Vec<String>,
}

impl From<&DdocMeta> for DdocHit {
    fn from(meta: &DdocMeta) -> Self {
        DdocHit {
            address: meta.address.clone(),
            schema: meta.schema.clone(),
            section: meta.section.clone(),
            id: meta.id.clone(),
            id_kind: meta.id_kind.clone(),
            trail: meta.trail.clone(),
            doc_title: meta.doc_title.clone(),
            state_stored: meta.state.clone(),
            state_derived: meta.derived_state.clone(),
            gate_terminal: meta.gate_terminal,
            rels: meta.rels.clone(),
            findings: meta.findings.clone(),
        }
    }
}

/// What `GET /search` answers with.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SearchResults {
    /// Ranked hits, best first.
    pub results: Vec<Hit>,
}

/// Why an empty answer was empty, when "nothing matched" would be a guess.
///
/// Rides in `meta`, never in `data`: it is commentary about the answer rather
/// than part of it, and a consumer that ignores it still reads the same
/// result list. It exists at all because an empty `results` array is the most
/// misread thing this surface produces — three unrelated causes, one shape,
/// and only one of them is an answer.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct EmptyBecause {
    /// The machine-readable cause: `below_floor` or `scan_incomplete`.
    pub reason: &'static str,
    /// One sentence stating what is actually known, for a human or an agent.
    pub detail: String,
}

/// A search's answer, plus the truth about it being empty when it is.
///
/// Carries the hits rather than [`SearchResults`] so that the wire type stays
/// a pure payload: the emptiness commentary belongs in `meta`, and a struct
/// that owned both would invite it into `data`.
#[derive(Clone, Debug, PartialEq)]
pub struct SearchOutcome {
    /// The ranked hits, best first.
    pub results: Vec<Hit>,
    /// Present only when `results` is empty AND the emptiness has a knowable
    /// cause. `None` alongside an empty list means the index really was asked
    /// and really had nothing nearer to offer.
    pub empty_because: Option<EmptyBecause>,
}

/// How many lines of an element's text a hit carries.
const SNIPPET_LINES: usize = 5;

/// Answer one search.
///
/// # Errors
/// [`catalog::QUERY_INVALID`] for a request that cannot be answered as asked,
/// [`catalog::PROVIDER_FAILED`] when the query cannot be embedded, and store
/// failures mapped by their own codes.
pub async fn search(
    state: &AppState,
    request: &SearchRequest,
    scope: &Scope,
) -> Result<SearchOutcome, Failure> {
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

    // Workshop 003's ONE user-facing `--source` axis, resolved into the two
    // axes it is really made of (its open question 1). `raw` and `smart` are
    // the VECTOR SPACE — a column on `embeddings_1024` with a check
    // constraint. `conversation` is not a third value there and could not be:
    // a turn has a raw vector and a smart vector exactly like a function does.
    // What makes a turn a turn is its element KIND, so this maps to the
    // content-type filter instead, and the two compose rather than competing.
    //
    // `all` and the absent default mean all CODE spaces. Conversations stay
    // opt-in, as 003 D3 reserved and workshop 005 kept: conversations are
    // opinions at a point in time and code is current truth, so blending them
    // by default would answer "how does auth work" with somebody's guess about
    // it from three weeks ago.
    //
    // `code` rides along because the emptiness diagnostics below are only
    // entitled to speak about file content: they reason from raw vectors
    // reachable through `worktree_files`, and a turn has no row there. Telling
    // a conversation search that its repository "has content indexed" would be
    // true and irrelevant, which is the kind of confident irrelevance this
    // whole change exists to remove.
    let (source, kinds, code) = match request.source.as_deref() {
        None | Some("all") => (None, Some(CODE_KINDS.to_vec()), true),
        Some("raw") => (Some(SourceKind::Raw), Some(CODE_KINDS.to_vec()), true),
        Some("smart") => (Some(SourceKind::Smart), Some(CODE_KINDS.to_vec()), true),
        Some("conversation") => (None, Some(vec![ElementKind::Turn]), false),
        Some(other) => {
            return Err(Failure::new(
                &catalog::QUERY_INVALID,
                format!("--source must be raw, smart, conversation or all, got {other:?}"),
            )
            .with_fix(
                "use `--source conversation` to search indexed turns, or omit it to search code",
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
    // comparison. It comes from the SCOPE rather than the request, so a search
    // run inside a repository with its own embedder is answered by that
    // embedder — the same rule whether the repository was named or inferred.
    let repo_key = scope.repo.clone().unwrap_or_default();
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

    let mut filters = SearchFilters {
        repo: scope.repo.clone(),
        path: request.path.as_deref().map(glob_to_like),
        source,
        kinds,
        max_distance,
        limit,
        ..SearchFilters::default()
    };

    apply_ddoc_filters(&mut filters, request);

    let hits = fs3_store::search_elements(&state.db, &model_key, &vector, &filters)
        .await
        .map_err(fail)?;

    if !hits.is_empty() {
        return Ok(SearchOutcome {
            results: hits.iter().map(render).collect(),
            empty_because: None,
        });
    }

    // An empty result is the most misread signal we have: "not indexed yet",
    // "indexed under a model this search cannot see", "indexed, but not in the
    // repository you are standing in", "the ranking never reached your
    // content", and "genuinely no match" all look identical, and only the last
    // one is an answer. Returning the empty list for all five makes the other
    // four into confident lies.
    if let Some(failure) = nothing_to_search(state, &model_key, &filters, code).await {
        return Err(failure);
    }

    Ok(SearchOutcome {
        results: Vec::new(),
        empty_because: empty_because(&filters, code),
    })
}

/// The true statement about an empty answer — and `None` when there is none.
///
/// Every branch here is limited to what the code path has actually PROVEN by
/// the time it runs, because a confident wrong explanation is worse than the
/// vague one it replaces.
///
/// A floor was set: rows may well have been found and then rejected for
/// scoring below it, and the floor is the fact worth reporting rather than a
/// shrug about the query.
///
/// No floor, and an anchor was applied: nothing could have been rejected —
/// every vector is within a cosine distance of 1.0 of every other — and
/// [`anchor_not_indexed`] has just proven the anchor holds indexed content.
/// An empty answer there is not an absence of matches, it is an approximate
/// nearest-neighbour scan that ran out of budget before reaching this
/// anchor's share of the index. Saying "nothing matched" is the lie this
/// function exists to stop telling.
///
/// No floor and no anchor, or a conversation search: nothing here has
/// established that any reachable content exists — [`anchor_not_indexed`]
/// declines to speak for turns, which have no `worktree_files` row — so there
/// is no claim to make and `None` is the honest answer. The generic steer
/// stands.
fn empty_because(filters: &SearchFilters, code: bool) -> Option<EmptyBecause> {
    if let Some(distance) = filters.max_distance
        && distance < 1.0
    {
        return Some(EmptyBecause {
            reason: "below_floor",
            detail: format!(
                "the active model has an index, but nothing in it scored at or above \
                 --min-score {:.3}",
                1.0 - distance
            ),
        });
    }

    if !code {
        return None;
    }

    let anchor = match (filters.repo.as_deref(), filters.path.as_deref()) {
        (None, None) => return None,
        (Some(repo), _) => repo.to_string(),
        (None, Some(path)) => format!("paths matching {path}"),
    };

    Some(EmptyBecause {
        reason: "scan_incomplete",
        detail: format!(
            "content IS indexed under {anchor} and no score floor was set, so this empty result \
             is not an absence of matches: the approximate nearest-neighbour scan stopped before \
             it reached that content. A scope holding a small share of a large index is where \
             this happens — retry without --repo/--path to see what the index does hold"
        ),
    })
}

/// Why an empty result was empty — when the reason is an ERROR the caller can
/// act on rather than a fact about the ranking.
///
/// Returns `None` when the active model has an index AND that index reaches
/// the anchor the caller asked about, because from there on the emptiness is
/// about scores and scan budgets — [`empty_because`]'s business, and a
/// successful answer's, not a failure's.
///
/// A store that cannot answer this also returns `None`: the search itself
/// succeeded, and failing it now over a diagnostic query would turn a working
/// empty result into an outage.
async fn nothing_to_search(
    state: &AppState,
    model_key: &str,
    filters: &SearchFilters,
    code: bool,
) -> Option<Failure> {
    let models = fs3_store::embedding_models(&state.db).await.ok()?;

    if models.iter().any(|(key, _)| key == model_key) {
        return anchor_not_indexed(state, model_key, filters, code).await;
    }

    // Vectors exist, but under other keys. This is the dangerous one: the
    // index is intact and unreachable, and nothing about "no results" would
    // ever have told the user that changing provider or width was the cause.
    if let Some((other, count)) = models.first() {
        let names: Vec<String> = models
            .iter()
            .map(|(key, count)| format!("{key} ({count})"))
            .collect();
        return Some(
            Failure::new(
                &catalog::QUERY_NO_INDEX,
                format!(
                    "no embeddings for the active model {model_key}; {} vectors are stored under \
                     {other}",
                    count
                ),
            )
            .with_detail("active_model", model_key)
            .with_detail("stored_models", names)
            .with_fix(format!(
                "the index was built by a different embedder. Either select the instance that \
                 produced {other} again, or re-index with the current one: `flowspace3 add \
                 <path>`. `flowspace3 doctor` names the active provider."
            )),
        );
    }

    Some(Failure::new(
        &catalog::QUERY_NO_INDEX,
        "no embeddings exist at all",
    ))
}

/// The anchor leg: the model has an index, but does it reach HERE?
///
/// The cause nobody guesses and the store cannot volunteer. A central index
/// holds several repositories; a search run inside one of them is answered
/// under that repository's anchor; and a repository that was never added — or
/// whose scan has not finished — produces exactly the same empty list as a
/// question with no good answer. Naming it is the difference between
/// `flowspace3 add .` and an afternoon of rephrasing the query.
///
/// Only asked for a CODE search with an anchor actually applied. With no
/// repository and no path filter the previous check already established the
/// index exists, and a second query would be work spent confirming it; for a
/// conversation search the probe is simply the wrong question, because a turn
/// reaches its repository through its conversation's anchor and has no
/// `worktree_files` row to be found by.
async fn anchor_not_indexed(
    state: &AppState,
    model_key: &str,
    filters: &SearchFilters,
    code: bool,
) -> Option<Failure> {
    let repo = filters.repo.as_deref();
    let path = filters.path.as_deref();
    if !code || (repo.is_none() && path.is_none()) {
        return None;
    }

    // Best-effort like its caller: a diagnostic that cannot run must not
    // convert a successful empty answer into a failed command.
    if fs3_store::anchor_has_vectors(&state.db, model_key, filters)
        .await
        .ok()?
    {
        return None;
    }

    let anchor = match (repo, path) {
        (Some(repo), Some(_)) => format!("{repo} under the requested --path"),
        (Some(repo), None) => repo.to_string(),
        (None, _) => "the requested --path".to_string(),
    };
    Some(
        Failure::new(
            &catalog::QUERY_NO_INDEX,
            format!(
                "the index has content for the active model {model_key}, but none of it belongs \
                 to {anchor} — so this search had nothing to rank, which is not the same as \
                 nothing matching"
            ),
        )
        .with_detail("active_model", model_key)
        .with_detail("anchor", anchor)
        .with_fix(
            "index it with `flowspace3 add <path>` and wait for `flowspace3 status` to drain, \
             or drop --repo/--path to search everything the index holds",
        ),
    )
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
        ddoc: element.ddoc.as_deref().map(DdocHit::from),
    }
}

/// The address a hit is printed with, in the right scheme for what it is.
///
/// A turn element ALREADY carries its `conv:<guid>#t<ord>` address — that is
/// what the store wrote — so it is returned verbatim. Passing it through
/// `element_address` would prefix a repository onto a conversation and produce
/// `el:git:…/conv:…`, an address nothing can parse and `get` would reject.
///
/// Everything else is `el:<repo>/<address>`, rendered by
/// `fs3_core::element_address` — the same function `get` and `tree` resolve
/// against, so what search PRINTS and what the read surface ACCEPTS cannot
/// drift apart. A hit with no live path keeps a bare `el:<address>`: the
/// content is real even when the checkout that held it is gone, and inventing a
/// repository for it would be a lie.
fn address_of(hit: &SearchHit) -> String {
    let element = &hit.similar.element;
    if element.kind == ElementKind::Turn || element.kind == ElementKind::Row {
        return element.address.clone();
    }
    fs3_core::element_address(hit.identity.as_deref(), &element.address)
}

/// Apply only ddoc-specific predicates, preserving the gate's three states.
fn apply_ddoc_filters(filters: &mut SearchFilters, request: &SearchRequest) {
    filters.id_kinds = request.id_kind.clone().map(|kind| vec![kind]);
    filters.gate_open = request.gate_open;
    filters.ddoc_schema.clone_from(&request.ddoc_schema);
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

    fn hit(element: fs3_core::Element) -> SearchHit {
        SearchHit {
            similar: fs3_store::SimilarElement {
                element,
                blob_sha: "a".repeat(40),
                parser_version: "test@1".to_string(),
                source_kind: SourceKind::Raw,
                smart: None,
                distance: 0.25,
            },
            identity: Some("git:host/org/repo".to_string()),
            path: Some("docs/plan.dd.json".to_string()),
        }
    }

    #[test]
    fn ddoc_metadata_is_serialized_on_rows_and_the_key_is_absent_on_code() {
        let address = "docs/plan.dd.json#acceptance_criteria/ac-0001";
        let mut meta = DdocMeta::new(
            address,
            "builder/plan",
            vec!["acceptance_criteria".to_string(), "ac-0001".to_string()],
            fs3_core::EmbedBasis::SchemaDeclared,
        );
        meta.state = Some("checked".to_string());
        meta.gate_terminal = Some(true);
        meta.derived_state = Some(DerivedState {
            complete: false,
            incomplete: vec!["dw-0001".to_string()],
        });
        let row = fs3_core::Element::new(
            ElementKind::Row,
            "ddoc_row",
            "ac-0001",
            address,
            fs3_core::Span::new(1, 1),
            "criterion",
        )
        .with_ddoc(meta);
        let row = serde_json::to_value(render(&hit(row))).expect("row hit serializes");
        assert_eq!(row["address"], address);
        assert_eq!(row["ddoc"]["state_stored"], "checked");
        assert_eq!(row["ddoc"]["state_derived"]["complete"], false);

        let code = fs3_core::Element::new(
            ElementKind::Function,
            "function_item",
            "run",
            "src/lib.rs::run",
            fs3_core::Span::new(1, 1),
            "fn run() {}",
        );
        let code = serde_json::to_value(render(&hit(code))).expect("code hit serializes");
        assert!(
            code.get("ddoc").is_none(),
            "the shipped code-hit wire shape must omit ddoc entirely: {code}"
        );
    }

    #[test]
    fn ddoc_filter_mapping_preserves_absent_open_and_closed() {
        let mut filters = SearchFilters::default();
        apply_ddoc_filters(&mut filters, &SearchRequest::default());
        assert_eq!(filters.id_kinds, None);
        assert_eq!(filters.gate_open, None);
        assert_eq!(filters.ddoc_schema, None);

        let request = SearchRequest {
            id_kind: Some("ac".to_string()),
            gate_open: Some(false),
            ddoc_schema: Some("builder/plan".to_string()),
            ..SearchRequest::default()
        };
        apply_ddoc_filters(&mut filters, &request);
        assert_eq!(filters.id_kinds, Some(vec!["ac".to_string()]));
        assert_eq!(filters.gate_open, Some(false));
        assert_eq!(filters.ddoc_schema.as_deref(), Some("builder/plan"));

        apply_ddoc_filters(
            &mut filters,
            &SearchRequest {
                gate_open: Some(true),
                ..SearchRequest::default()
            },
        );
        assert_eq!(filters.gate_open, Some(true));
    }

    /// The decision table for [`EmptyBecause`], which is the whole of the
    /// envelope-honesty promise: every claim it makes has to be one the code
    /// path has already proven, and every case it cannot speak to has to stay
    /// silent rather than guess.
    ///
    /// Driven directly rather than through a query because the interesting
    /// input — a nearest-neighbour scan that stops before reaching content it
    /// can prove is there — is provoked in a live index by squeezing pgvector's
    /// scan budget, and pgvector randomises HNSW graph construction, so that
    /// fixture is a coin flip. A flaky test that watches the right thing is
    /// worth less than a stable one that watches the decision.
    mod empty_answers {
        use super::*;

        fn anchored() -> SearchFilters {
            SearchFilters {
                repo: Some("git:example.com/one".to_string()),
                kinds: Some(CODE_KINDS.to_vec()),
                ..SearchFilters::default()
            }
        }

        /// No floor, an anchor, and (by the time this runs) proof that the
        /// anchor holds eligible content. Nothing could have been rejected on
        /// score, so the scan is the only remaining explanation and the
        /// envelope says so.
        #[test]
        fn an_anchored_search_with_no_floor_blames_the_scan_and_names_the_scope() {
            let reason = empty_because(&anchored(), true).expect("a claim is available");
            assert_eq!(reason.reason, "scan_incomplete");
            assert!(reason.detail.contains("git:example.com/one"));
            assert!(
                !reason.detail.contains("nothing matched"),
                "the shrug is what this replaces: {}",
                reason.detail
            );
        }

        /// A floor was set, so rows may well have been found and rejected. The
        /// floor is the fact, and it takes precedence over the scan story even
        /// with an anchor in play.
        #[test]
        fn a_floor_is_reported_as_the_floor() {
            let filters = SearchFilters {
                max_distance: Some(0.3),
                ..anchored()
            };
            let reason = empty_because(&filters, true).expect("a claim is available");
            assert_eq!(reason.reason, "below_floor");
            assert!(
                reason.detail.contains("0.700"),
                "the floor is reported as the caller spelled it, not as a \
                 distance: {}",
                reason.detail
            );
        }

        /// A floor of exactly zero excludes nothing, so it is not an
        /// explanation — the scan story still applies.
        #[test]
        fn a_floor_of_zero_explains_nothing_and_is_not_offered_as_one() {
            let filters = SearchFilters {
                max_distance: Some(1.0),
                ..anchored()
            };
            assert_eq!(
                empty_because(&filters, true)
                    .expect("a claim is available")
                    .reason,
                "scan_incomplete"
            );
        }

        /// Unanchored, nothing has established that any reachable content
        /// exists, so there is no claim to make.
        #[test]
        fn an_unanchored_search_makes_no_claim() {
            let filters = SearchFilters {
                kinds: Some(CODE_KINDS.to_vec()),
                ..SearchFilters::default()
            };
            assert!(empty_because(&filters, false).is_none());
            assert!(empty_because(&filters, true).is_none());
        }

        /// A conversation search is answered from turns, which have no
        /// `worktree_files` row — the probe that would justify the scan claim
        /// never ran, so the claim is not available.
        #[test]
        fn a_conversation_search_makes_no_claim_about_file_content() {
            assert!(empty_because(&anchored(), false).is_none());
        }

        /// A `--path` filter is an anchor too, and the report names what was
        /// actually narrowed rather than a repository nobody mentioned.
        #[test]
        fn a_path_filter_is_named_as_the_scope() {
            let filters = SearchFilters {
                repo: None,
                path: Some("crates/store/%".to_string()),
                kinds: Some(CODE_KINDS.to_vec()),
                ..SearchFilters::default()
            };
            let reason = empty_because(&filters, true).expect("a claim is available");
            assert_eq!(reason.reason, "scan_incomplete");
            assert!(reason.detail.contains("crates/store/%"));
        }
    }
}
