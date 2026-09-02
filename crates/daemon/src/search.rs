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
//!
//! # Snap-in recipe
//!
//! No new configuration, constructor, or route registration is required.
//! The existing HTTP handler resolves `Scope` once, passes it unchanged to
//! [`search`], and attaches the returned limit/weak-match facts to envelope
//! metadata. The store call must receive both `scope.repo` and
//! `scope.worktree`; dropping the latter restores cross-checkout leakage.

use fs3_core::catalog;
use fs3_core::envelope::Failure;
use fs3_core::{ConversationId, DdocAddress, DdocMeta, Element, ElementKind};
use fs3_store::{LexicalHit, PathFilterProbe, SearchFilters, SearchHit};
use serde::{Deserialize, Serialize};

use crate::runner::fail;
use crate::scope::Scope;
use crate::wiring::AppState;
use fs3_core::views::search::{DdocHit, Hit, SearchChannel, SearchComposition};

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
    /// Content source: `code`, `doc`, `conversation`, or `all` (the default).
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

/// A best hit below this score is weak enough to warrant an advisory hint.
///
/// This is not a probability. Absolute similarity depends on both corpus and
/// embedder. The value was calibrated on 2026-08-28 against the live
/// flowspace3 index using Azure `text-embedding-3-small-no-rate` at 1024
/// dimensions. The snapshot separated known-relevant from known-noise queries:
///
/// | expected | query | best score |
/// |---|---|---:|
/// | relevant | claim queued job once deduplicate key | 0.6985 |
/// | relevant | remove root dereference worktree files | 0.6456 |
/// | relevant | resolve caller cwd registered worktree scope | 0.6146 |
/// | relevant | known relevant, mediocre matches | 0.5509–0.5554 |
/// | noise | known irrelevant matches | 0.4431–0.4644 |
/// | noise | quantum chromodynamics gluon confinement | 0.3118 |
///
/// The durable part is the labelled-query procedure, not these samples: the
/// index grows, and changing either corpus or embedder invalidates the floor.
/// False warnings teach callers to ignore the hint, so new relevant evidence
/// below the current band moves this floor down, never up by taste.
pub const WEAK_MATCH_SCORE_FLOOR: f64 = 0.50;

/// Closed source groups. New element kinds require an explicit placement.
const CODE_KINDS: [ElementKind; 3] = [
    ElementKind::File,
    ElementKind::Container,
    ElementKind::Function,
];
const DOC_KINDS: [ElementKind; 2] = [ElementKind::Section, ElementKind::Row];
const FILE_KINDS: [ElementKind; 5] = [
    ElementKind::File,
    ElementKind::Container,
    ElementKind::Function,
    ElementKind::Section,
    ElementKind::Row,
];
const ALL_KINDS: [ElementKind; 6] = [
    ElementKind::File,
    ElementKind::Container,
    ElementKind::Function,
    ElementKind::Section,
    ElementKind::Row,
    ElementKind::Turn,
];

fn weak_match_score(best: Option<f64>) -> bool {
    best.is_some_and(|score| score < WEAK_MATCH_SCORE_FLOOR)
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
    /// The machine-readable cause: `below_floor`, `scan_incomplete`, or `path_unmatched`.
    pub reason: &'static str,
    /// One sentence stating what is actually known, for a human or an agent.
    pub detail: String,
    /// A concrete correction when the index can expose one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

/// A search's answer, plus the truth about it being empty when it is.
///
/// Carries the hits rather than [`SearchResults`] so that the wire type stays
/// a pure payload: the emptiness commentary belongs in `meta`, and a struct
/// that owned both would invite it into `data`.
///
/// [`Hit`] and [`SearchResults`] themselves live in `fs3_core::views::search`
/// since tk-a106 — a consumer must be able to read a payload without depending
/// on this crate — so this file uses them rather than defining them.
#[derive(Clone, Debug, PartialEq)]
pub struct SearchOutcome {
    /// The ranked hits, best first.
    pub results: Vec<Hit>,
    /// Counts from the same thresholded scored set, before display truncation.
    pub composition: SearchComposition,
    /// Present only when `results` is empty AND the emptiness has a knowable
    /// cause. `None` alongside an empty list means the index really was asked
    /// and really had nothing nearer to offer.
    pub empty_because: Option<EmptyBecause>,
    /// The caller-visible result cap.
    pub limit: i64,
    /// Whether at least one additional legitimate result existed beyond the cap.
    pub truncated: bool,
    /// Whether semantic candidate expansion ended at its bounded ceiling or
    /// after the admitted set stopped growing.
    pub candidate_limit_exhausted: bool,
    /// Semantic search returned a bounded short page. Independent of
    /// `empty_because`, lexical fusion, and display truncation.
    pub scan_incomplete: bool,
    /// Number of semantic candidate pages examined.
    pub passes: usize,
}

impl SearchOutcome {
    /// Whether the best available result falls below the calibrated floor.
    #[must_use]
    pub fn is_weak_match(&self) -> bool {
        weak_match_score(self.results.first().map(|hit| hit.score))
    }
}

/// Resolve the shared `--source` corpus contract used by search and ask.
///
/// Keeping validation here means the two verbs cannot accept different source
/// spellings or classify a turn differently.
pub(crate) fn source_filter(
    source: Option<&str>,
) -> Result<(Option<Vec<ElementKind>>, bool), Failure> {
    match source {
        None | Some("all") => Ok((Some(ALL_KINDS.to_vec()), true)),
        Some("code") => Ok((Some(CODE_KINDS.to_vec()), true)),
        Some("doc") => Ok((Some(DOC_KINDS.to_vec()), true)),
        Some("conversation") => Ok((Some(vec![ElementKind::Turn]), false)),
        Some(other) => Err(Failure::new(
            &catalog::QUERY_INVALID,
            format!("--source must be code, doc, conversation or all, got {other:?}"),
        )
        .with_fix("use `--source conversation` to search only indexed turns, or omit it to search every source")),
    }
}

/// Element kinds reachable through a file path filter.
pub(crate) fn path_source_filter(source: Option<&str>) -> Result<Vec<ElementKind>, Failure> {
    match source {
        None | Some("all") => Ok(FILE_KINDS.to_vec()),
        Some("code") => Ok(CODE_KINDS.to_vec()),
        Some("doc") => Ok(DOC_KINDS.to_vec()),
        Some("conversation") => Err(Failure::new(
            &catalog::QUERY_INVALID,
            "--path conflicts with --source conversation because conversations carry no file path",
        )
        .with_fix(
            "remove --path and use --repo to scope conversations, or select --source code or doc",
        )),
        Some(other) => Err(Failure::new(
            &catalog::QUERY_INVALID,
            format!("--source must be code, doc, conversation or all, got {other:?}"),
        )),
    }
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
    search_filtered(state, request, scope, None).await
}

/// Answer one search pinned to a single transcript.
///
/// Kept separate from [`SearchRequest`] so the public search endpoint does not
/// accidentally acquire a second spelling for ask's `--conversation` scope.
pub(crate) async fn search_in_conversation(
    state: &AppState,
    request: &SearchRequest,
    scope: &Scope,
    conversation: &ConversationId,
) -> Result<SearchOutcome, Failure> {
    search_filtered(state, request, scope, Some(conversation)).await
}

async fn search_filtered(
    state: &AppState,
    request: &SearchRequest,
    scope: &Scope,
    conversation: Option<&ConversationId>,
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

    // `source` is the content corpus. The absent/default and `all` search the
    // complete corpus; narrower values keep ranking identical while selecting
    // one stable source group before the scored-set limit.
    let (mut kinds, file_backed) = source_filter(request.source.as_deref())?;
    if request.path.is_some() {
        kinds = Some(path_source_filter(request.source.as_deref())?);
    }

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
        worktree: scope.worktree.clone(),
        conversation: conversation.map(|guid| guid.as_str().to_string()),
        path: request.path.as_deref().map(glob_to_like),
        source: None,
        kinds,
        max_distance,
        limit: MAX_LIMIT + 1,
        ..SearchFilters::default()
    };

    apply_ddoc_filters(&mut filters, request);

    let (semantic, mut lexical_hits) = tokio::try_join!(
        fs3_store::search_elements(&state.db, &model_key, &vector, &filters),
        fs3_store::search_lexical(&state.db, query, &filters),
    )
    .map_err(fail)?;
    let candidate_limit_exhausted = semantic.candidate_limit_exhausted;
    let scan_incomplete = candidate_limit_exhausted;
    let passes = semantic.passes;
    let mut hits = semantic.hits;

    // Both store legs scope candidate eligibility and representative
    // resolution. This final legitimacy guard is deliberately shared: a future
    // SQL change may not emit a repo-less address from a scoped search.
    let scoped = scope.repo.is_some() || scope.worktree.is_some() || request.path.is_some();
    hits.retain(|hit| {
        let resolved = hit.identity.is_some() || hit.root_path.is_some() || hit.path.is_some();
        if scoped && !resolved {
            tracing::warn!(
                raw_hash = %hit.similar.element.raw_hash(),
                repo = ?scope.repo,
                path = ?request.path,
                worktree = ?scope.worktree,
                "scoped semantic representative resolved without live provenance; dropping hit"
            );
        }
        !scoped || resolved
    });
    lexical_hits.retain(|hit| {
        let resolved = hit.identity.is_some() || hit.root_path.is_some() || hit.path.is_some();
        if scoped && !resolved {
            tracing::warn!(
                raw_hash = %hit.element.raw_hash(),
                repo = ?scope.repo,
                path = ?request.path,
                worktree = ?scope.worktree,
                "scoped lexical representative resolved without live provenance; dropping hit"
            );
        }
        !scoped || resolved
    });

    let mut ranked = fuse(
        lexical_hits.iter().map(render_lexical),
        hits.iter().map(render),
    );
    let composition = composition(&ranked);
    let truncated = ranked.len() > limit as usize;
    if !ranked.is_empty() {
        ranked.truncate(limit as usize);
        return Ok(SearchOutcome {
            results: ranked,
            composition,
            empty_because: None,
            limit,
            truncated,
            candidate_limit_exhausted,
            scan_incomplete,
            passes,
        });
    }

    if let Some(reason) = path_unmatched(state, request, &filters, file_backed).await {
        return Ok(SearchOutcome {
            results: Vec::new(),
            composition: SearchComposition::default(),
            empty_because: Some(reason),
            limit,
            truncated: false,
            candidate_limit_exhausted,
            scan_incomplete,
            passes,
        });
    }

    // An empty result is the most misread signal we have: "not indexed yet",
    // "indexed under a model this search cannot see", "indexed, but not in the
    // repository you are standing in", "the ranking never reached your
    // content", and "genuinely no match" all look identical, and only the last
    // one is an answer. Returning the empty list for all five makes the other
    // four into confident lies.
    if let Some(failure) = nothing_to_search(state, &model_key, &filters, file_backed).await {
        return Err(failure);
    }

    Ok(SearchOutcome {
        results: Vec::new(),
        composition: SearchComposition::default(),
        empty_because: empty_because(&filters, file_backed, scan_incomplete),
        limit,
        truncated: false,
        candidate_limit_exhausted,
        scan_incomplete,
        passes,
    })
}

fn composition(hits: &[Hit]) -> SearchComposition {
    let mut counts = SearchComposition::default();
    for hit in hits {
        match hit.kind.as_str() {
            "turn" => counts.conversation += 1,
            "section" | "row" => counts.doc += 1,
            _ => counts.code += 1,
        }
    }
    counts
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
/// No floor, an anchor, and NO selective content predicate: nothing could have
/// been rejected — every vector is within a cosine distance of 1.0 of every
/// other — and [`anchor_not_indexed`] has just proven the anchor holds indexed
/// content. An empty answer there is not an absence of matches, it is an
/// approximate nearest-neighbour scan that ran out of budget before reaching
/// this anchor's share of the index.
///
/// Source and ddoc predicates can legitimately admit zero rows. In that case
/// the scan cause is unproven, so `None` keeps the generic filtered-empty steer
///
/// No floor and no anchor, or a conversation search: nothing here has
/// established that any reachable content exists — [`anchor_not_indexed`]
/// declines to speak for turns, which have no `worktree_files` row — so there
/// is no claim to make and `None` is the honest answer. The generic steer
/// stands.
fn empty_because(
    filters: &SearchFilters,
    code: bool,
    scan_incomplete: bool,
) -> Option<EmptyBecause> {
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
            hint: None,
        });
    }

    if !code {
        return None;
    }

    if !scan_incomplete
        && (filters.source.is_some()
            || filters.id_kinds.is_some()
            || filters.gate_open.is_some()
            || filters.ddoc_schema.is_some())
    {
        return None;
    }

    let anchor = match (
        filters.worktree.as_deref(),
        filters.repo.as_deref(),
        filters.path.as_deref(),
    ) {
        (Some(worktree), Some(repo), Some(path)) => {
            format!("checkout {worktree} of {repo} under paths matching {path}")
        }
        (Some(worktree), Some(repo), None) => format!("checkout {worktree} of {repo}"),
        (Some(worktree), None, _) => format!("checkout {worktree}"),
        (None, Some(repo), _) => repo.to_string(),
        (None, None, Some(path)) => format!("paths matching {path}"),
        (None, None, None) => return None,
    };

    Some(EmptyBecause {
        reason: "scan_incomplete",
        detail: format!(
            "content IS indexed under {anchor} and no score floor was set, so this empty result \
             is not an absence of matches: the approximate nearest-neighbour scan stopped before \
             it reached that content. A scope holding a small share of a large index is where \
             this happens — retry without --repo/--path to see what the index does hold"
        ),
        hint: None,
    })
}

/// Diagnose a path filter that cannot reach any indexed path in its ownership scope.
async fn path_unmatched(
    state: &AppState,
    request: &SearchRequest,
    filters: &SearchFilters,
    code: bool,
) -> Option<EmptyBecause> {
    if !code {
        return None;
    }
    let requested = request.path.as_deref()?;
    let pattern = filters.path.as_deref()?;
    let probe = fs3_store::path_filter_probe(
        &state.db,
        filters.repo.as_deref(),
        filters.worktree.as_deref(),
        pattern,
        filters.kinds.as_deref(),
    )
    .await
    .ok()?;
    path_unmatched_reason(requested, probe)
}

/// Explain why a path glob cannot reach any indexed file in its ownership scope.
pub(crate) fn path_unmatched_reason(
    requested: &str,
    probe: PathFilterProbe,
) -> Option<EmptyBecause> {
    if probe.matches || probe.top_level_entries.is_empty() {
        return None;
    }

    let mut entries = probe.top_level_entries;
    const LAYOUT_LIMIT: usize = 12;
    let omitted = entries.len().saturating_sub(LAYOUT_LIMIT);
    entries.truncate(LAYOUT_LIMIT);
    let mut layout = entries.join(", ");
    if omitted > 0 {
        layout.push_str(&format!(", and {omitted} more"));
    }

    Some(EmptyBecause {
        reason: "path_unmatched",
        detail: format!(
            "the --path filter {requested:?} matches zero indexed paths in this scope; this says \
             nothing about whether the requested code exists elsewhere in the indexed layout"
        ),
        hint: Some(format!(
            "indexed top-level entries in this scope: {layout}; correct --path to start from one \
             of these entries"
        )),
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
/// repository, worktree, or path filter the previous check already established
/// the index exists, and a second query would be work spent confirming it; for
/// a conversation search the probe is simply the wrong question, because a
/// turn reaches its repository through its conversation's anchor and has no
/// `worktree_files` row to be found by.
async fn anchor_not_indexed(
    state: &AppState,
    model_key: &str,
    filters: &SearchFilters,
    code: bool,
) -> Option<Failure> {
    let repo = filters.repo.as_deref();
    let worktree = filters.worktree.as_deref();
    let path = filters.path.as_deref();
    if !code || (repo.is_none() && worktree.is_none() && path.is_none()) {
        return None;
    }

    // Best-effort like its caller: a diagnostic that cannot run must not
    // convert a successful empty answer into a failed command.
    let scope = fs3_store::AnchorScope {
        repo,
        worktree,
        path,
    };
    if fs3_store::anchor_has_vectors(&state.db, model_key, &scope)
        .await
        .ok()?
    {
        return None;
    }

    let anchor = match (worktree, repo, path) {
        (Some(worktree), Some(repo), Some(_)) => {
            format!("checkout {worktree} of {repo} under the requested --path")
        }
        (Some(worktree), Some(repo), None) => format!("checkout {worktree} of {repo}"),
        (Some(worktree), None, _) => format!("checkout {worktree}"),
        (None, Some(repo), Some(_)) => format!("{repo} under the requested --path"),
        (None, Some(repo), None) => repo.to_string(),
        (None, None, _) => "the requested --path".to_string(),
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
             or use `--repo all` to search everything the index holds",
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
        channel: SearchChannel::Semantic,
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
        worktree: hit.root_path.clone(),
        ddoc: element
            .ddoc
            .as_deref()
            .map(|meta| ddoc_hit(meta, hit.path.as_deref())),
    }
}

fn render_lexical(hit: &LexicalHit) -> Hit {
    let element = &hit.element;
    Hit {
        address: address_of_element(element, hit.identity.as_deref(), hit.path.as_deref()),
        // Exact substring identity is the lexical score; this is not cosine.
        score: 1.0,
        channel: SearchChannel::Lexical,
        match_field: format!("exact_{}", hit.matched.as_str()),
        kind: element.kind.as_str().to_string(),
        subkind: element.subkind.clone(),
        name: element.name.clone(),
        span: [element.span.start_line, element.span.end_line],
        snippet: snippet(&element.raw_text),
        smart: None,
        tags: Vec::new(),
        repo: hit.identity.clone(),
        path: hit.path.clone(),
        worktree: hit.root_path.clone(),
        ddoc: element
            .ddoc
            .as_deref()
            .map(|meta| ddoc_hit(meta, hit.path.as_deref())),
    }
}

/// Lexical order is authoritative. A semantic duplicate changes only the
/// channel label; it never changes lexical placement or manufactures a score.
fn fuse(
    lexical: impl IntoIterator<Item = Hit>,
    semantic: impl IntoIterator<Item = Hit>,
) -> Vec<Hit> {
    let mut fused: Vec<Hit> = Vec::new();
    for hit in lexical {
        if !fused.iter().any(|existing| same_hit(existing, &hit)) {
            fused.push(hit);
        }
    }
    for hit in semantic {
        if let Some(existing) = fused.iter_mut().find(|existing| same_hit(existing, &hit)) {
            if existing.channel == SearchChannel::Lexical {
                existing.channel = SearchChannel::Both;
            }
        } else {
            fused.push(hit);
        }
    }
    fused
}

fn same_hit(left: &Hit, right: &Hit) -> bool {
    left.address == right.address
        && left.worktree == right.worktree
        && left.path == right.path
        && left.span == right.span
}

/// Map stored ddoc metadata into the consumer-owned search view.
fn ddoc_hit(meta: &DdocMeta, path: Option<&str>) -> DdocHit {
    DdocHit {
        address: rebase_ddoc_address(&meta.address, path),
        schema: meta.schema.clone(),
        section: meta.section.clone(),
        id: meta.id.clone(),
        id_kind: meta.id_kind.clone(),
        trail: meta.trail.clone(),
        doc_title: meta.doc_title.clone(),
        embed_basis: meta.embed_basis,
        state_stored: meta.state.clone(),
        state_derived: meta.derived_state.clone(),
        gate_terminal: meta.gate_terminal,
        rels: meta.rels.clone(),
        findings: meta.findings.clone(),
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
    address_of_element(
        &hit.similar.element,
        hit.identity.as_deref(),
        hit.path.as_deref(),
    )
}

fn address_of_element(element: &Element, identity: Option<&str>, path: Option<&str>) -> String {
    if element.kind == ElementKind::Turn {
        return element.address.clone();
    }
    if element.kind == ElementKind::Row {
        return rebase_ddoc_address(&element.address, path);
    }
    let rebased = path.and_then(|path| {
        let suffix = element
            .address
            .find("::")
            .map_or("", |index| &element.address[index..]);
        let stored_path = element
            .address
            .strip_suffix(suffix)
            .unwrap_or(&element.address);
        (stored_path != path).then(|| format!("{path}{suffix}"))
    });
    fs3_core::element_address(identity, rebased.as_deref().unwrap_or(&element.address))
}

fn rebase_ddoc_address(address: &str, path: Option<&str>) -> String {
    let Some(path) = path else {
        return address.to_string();
    };
    DdocAddress::parse(address).map_or_else(
        |_| address.to_string(),
        |mut address| {
            address.file = path.to_string();
            address.render()
        },
    )
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
pub(crate) fn glob_to_like(glob: &str) -> String {
    let ends_with_wildcard = glob.ends_with('*');
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
            // SQL LIKE treats backslash as its escape. Escape it alongside its
            // metacharacters so search and the in-process get guard agree.
            '\\' | '%' | '_' => {
                out.push('\\');
                out.push(ch);
            }
            other => out.push(other),
        }
    }
    // A bare prefix behaves the way a person expects a path filter to.
    if !ends_with_wildcard {
        out.push('%');
    }
    out
}

/// Match one resolved repository-relative path with search's glob semantics.
pub(crate) fn path_matches_glob(path: &str, glob: &str) -> bool {
    let text: Vec<char> = path.chars().collect();
    let mut pattern: Vec<char> = glob.chars().collect();
    if pattern.last() != Some(&'*') {
        pattern.push('*');
    }

    let mut previous = vec![false; text.len() + 1];
    let mut current = vec![false; text.len() + 1];
    previous[0] = true;

    for token in pattern {
        current.fill(false);
        if token == '*' {
            current[0] = previous[0];
        }
        for index in 1..=text.len() {
            current[index] = if token == '*' {
                previous[index] || current[index - 1]
            } else {
                previous[index - 1] && (token == '?' || token == text[index - 1])
            };
        }
        std::mem::swap(&mut previous, &mut current);
    }

    previous[text.len()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use fs3_core::DerivedState;
    use fs3_store::SourceKind;

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
        assert_eq!(glob_to_like("100%"), "100\\%%");
        assert_eq!(glob_to_like(r"a\b"), r"a\\b%");
    }

    #[test]
    fn resolved_paths_use_the_same_prefix_wildcard_semantics_as_search() {
        assert!(path_matches_glob(
            "crates/store/src/lib.rs",
            "crates/store/**"
        ));
        assert!(path_matches_glob("crates/store/src/lib.rs", "crates/store"));
        assert!(path_matches_glob("crates/λ.rs", "crates/?.rs"));
        assert!(!path_matches_glob(
            "crates/daemon/src/lib.rs",
            "crates/store/**"
        ));
        assert!(!path_matches_glob("myXfile.rs", "my_file.rs"));
        assert!(path_matches_glob(r"a\b/file.rs", r"a\b"));
        assert!(!path_matches_glob("ab/file.rs", r"a\b"));
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

    fn ranked(address: &str, channel: SearchChannel, score: f64) -> Hit {
        Hit {
            address: address.to_string(),
            score,
            channel,
            match_field: match channel {
                SearchChannel::Lexical | SearchChannel::Both => "exact_text",
                SearchChannel::Semantic => "raw",
            }
            .to_string(),
            kind: "function".to_string(),
            subkind: "function_item".to_string(),
            name: address.to_string(),
            span: [1, 1],
            snippet: String::new(),
            smart: None,
            tags: Vec::new(),
            repo: None,
            path: None,
            worktree: None,
            ddoc: None,
        }
    }

    #[test]
    fn fusion_keeps_lexical_placement_and_score_for_both() {
        let hits = fuse(
            [ranked("exact", SearchChannel::Lexical, 1.0)],
            [
                ranked("semantic-first", SearchChannel::Semantic, 0.99),
                ranked("exact", SearchChannel::Semantic, 0.25),
            ],
        );
        assert_eq!(hits[0].address, "exact");
        assert_eq!(hits[0].channel, SearchChannel::Both);
        assert_eq!(hits[0].score, 1.0);
        assert_eq!(hits[1].address, "semantic-first");
    }

    #[test]
    fn either_leg_is_visible_when_the_other_is_empty() {
        let lexical = fuse(
            [ranked("lexical", SearchChannel::Lexical, 1.0)],
            std::iter::empty(),
        );
        assert_eq!(lexical[0].channel, SearchChannel::Lexical);

        let semantic = fuse(
            std::iter::empty(),
            [ranked("semantic", SearchChannel::Semantic, 0.75)],
        );
        assert_eq!(semantic[0].channel, SearchChannel::Semantic);
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
            root_path: Some("/checkout".to_string()),
            path: Some("docs/plan.dd.json".to_string()),
        }
    }

    #[test]
    fn ddoc_metadata_is_serialized_on_rows_and_the_key_is_absent_on_code() {
        let address = "docs/owner.dd.json#acceptance_criteria/ac-0001";
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
        assert_eq!(
            row["address"],
            "docs/plan.dd.json#acceptance_criteria/ac-0001"
        );
        assert_eq!(
            row["ddoc"]["address"],
            "docs/plan.dd.json#acceptance_criteria/ac-0001"
        );
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
    fn embed_basis_surfaces_schema_declared_and_fallback_rows_only() {
        let source = br#"{
            "dd": {"schema": "builder/plan"},
            "sections": [{"name": "acceptance_criteria", "value": [
                {"id": "ac-0001", "claim": "Visible basis", "state": "unchecked"}
            ]}]
        }"#;
        let facts = fs3_core::DdocSchemaFacts {
            schema: "builder/plan".to_string(),
            prose_fields: std::collections::BTreeMap::from([(
                "acceptance_criteria".to_string(),
                vec!["claim".to_string()],
            )]),
            ..fs3_core::DdocSchemaFacts::default()
        };
        let row = |facts| {
            fs3_parsers::scan_ddoc(std::path::Path::new("docs/plan.dd.json"), source, facts)
                .expect("ddoc parses")
                .root
                .iter()
                .find(|element| element.kind == ElementKind::Row)
                .expect("row exists")
                .clone()
        };

        let declared = serde_json::to_value(render(&hit(row(Some(&facts)))))
            .expect("schema-declared row serializes");
        assert_eq!(declared["ddoc"]["embed_basis"], "schema_declared");

        let fallback =
            serde_json::to_value(render(&hit(row(None)))).expect("fallback row serializes");
        assert_eq!(fallback["ddoc"]["embed_basis"], "fallback");

        let code = fs3_core::Element::new(
            ElementKind::Function,
            "function_item",
            "run",
            "src/lib.rs::run",
            fs3_core::Span::new(1, 1),
            "fn run() {}",
        );
        let code = serde_json::to_value(render(&hit(code))).expect("code hit serializes");
        assert!(code.get("ddoc").is_none());
        assert!(!code.to_string().contains("embed_basis"));
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

    #[test]
    fn weak_match_is_advisory_only_below_the_calibrated_floor() {
        assert!(weak_match_score(Some(WEAK_MATCH_SCORE_FLOOR - 0.01)));
        assert!(!weak_match_score(Some(WEAK_MATCH_SCORE_FLOOR)));
        assert!(!weak_match_score(Some(WEAK_MATCH_SCORE_FLOOR + 0.01)));
        assert!(
            !weak_match_score(None),
            "zero results use their existing steer"
        );
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
            let reason = empty_because(&anchored(), true, false).expect("a claim is available");
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
            let reason = empty_because(&filters, true, false).expect("a claim is available");
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
                empty_because(&filters, true, false)
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
            assert!(empty_because(&filters, false, false).is_none());
            assert!(empty_because(&filters, true, false).is_none());
        }

        /// A conversation search is answered from turns, which have no
        /// `worktree_files` row — the probe that would justify the scan claim
        /// never ran, so the claim is not available.
        #[test]
        fn a_conversation_search_makes_no_claim_about_file_content() {
            assert!(empty_because(&anchored(), false, false).is_none());
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
            let reason = empty_because(&filters, true, false).expect("a claim is available");
            assert_eq!(reason.reason, "scan_incomplete");
            assert!(reason.detail.contains("crates/store/%"));
        }
    }
}
