//! What `search` answers with.

use serde::{Deserialize, Serialize};

/// Deterministic-document metadata for a row hit. Absent on code hits.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DdocHit {
    /// dd's path-qualified positional address; paste directly into `ddocs get`.
    pub address: String,
    /// The document's declared schema, verbatim.
    pub schema: String,
    /// The section containing the row.
    pub section: String,
    /// The row's permanent id.
    pub id: String,
    /// The raw minted-id prefix, when one exists.
    pub id_kind: Option<String>,
    /// The complete positional trail from section to row.
    pub trail: Vec<String>,
    /// The document title, when declared.
    pub doc_title: Option<String>,
    /// How the parser chose embedded text: schema-declared fields or fallback.
    pub embed_basis: crate::EmbedBasis,
    /// The source document's stored state. It is not authoritative.
    pub state_stored: Option<String>,
    /// State derived from assertions. Believe this claim when present.
    pub state_derived: Option<crate::DerivedState>,
    /// Whether the stored state belongs to the schema's terminal set.
    pub gate_terminal: Option<bool>,
    /// Typed outbound dd relations.
    pub rels: Vec<crate::DdocRel>,
    /// Validation findings attached without suppressing the row.
    pub findings: Vec<String>,
}

/// Retrieval channel responsible for a hit's placement.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchChannel {
    /// Vector similarity only.
    #[default]
    Semantic,
    /// Indexed exact text only.
    Lexical,
    /// Both legs found it; lexical placement and score win.
    Both,
}

/// One hit, in the workshop-003 row shape.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Hit {
    /// `el:<repo>/<path>::<container>::<name>` — the universal currency (D7).
    pub address: String,
    /// 1.0 is identical; highest first.
    pub score: f64,
    /// Which retrieval leg found this hit.
    #[serde(default)]
    pub channel: SearchChannel,
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
    /// The registered worktree root that supplied this hit.
    pub worktree: Option<String>,
    /// Deterministic-document metadata. Deliberately absent, including the key,
    /// on code hits so the shipped code-hit envelope stays unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ddoc: Option<DdocHit>,
    /// Who had the conversation a turn hit came from: harness, models, pij
    /// seat. Absent, including the key, on every other hit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<crate::ConversationAgent>,
    /// Hits from this one's conversation or file that ranked high enough for
    /// this page but were moved below it, so one source cannot fill the page.
    /// On the top hit of that conversation or file only; absent otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub balanced: Option<Balanced>,
}

/// What balancing moved down from one conversation or file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Balanced {
    /// `conversation` or `file`.
    pub by: String,
    /// Hits from it that ranked within this page or earlier and were moved
    /// below. They are not dropped: later pages, or `see_all`, show them.
    pub moved_down: usize,
    /// The same search with balancing off: the raw ranking.
    pub see_all: String,
}

/// Counts from the scored set before the caller's display limit is applied.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchComposition {
    /// Source-code declarations and containers.
    pub code: u64,
    /// Prose sections and deterministic-document rows.
    pub doc: u64,
    /// Conversation turns.
    pub conversation: u64,
}

/// What `GET /search` answers with.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SearchResults {
    /// Ranked hits, best first.
    pub results: Vec<Hit>,
    /// Number of ranked hits skipped before this page.
    #[serde(default)]
    pub offset: i64,
    /// Start of the next page when this page filled the requested limit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_offset: Option<i64>,
    /// Source totals within the active score threshold, before top-k truncation.
    #[serde(default)]
    pub composition: SearchComposition,
    /// Which repositories this page was drawn from.
    #[serde(default)]
    pub scope: SearchScope,
}

/// Where one search's time went, in whole milliseconds.
///
/// The store legs run concurrently, so `store_ms` is the wall time of the
/// slowest of them, not their sum. The serial stages add up to about
/// `total_ms`: guard, scope, embed, store, rank, agents and diagnose.
///
/// A search used to log its split only past five seconds, so a four-second
/// search left no record at all (2026-10-14). Every search now carries this
/// in `meta.timing`, and `status` keeps percentiles over the recent ones.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchTiming {
    /// From the request arriving to the page being ready.
    pub total_ms: u64,
    /// The schema compatibility check.
    pub guard_ms: u64,
    /// Working out which repository the caller means.
    pub scope_ms: u64,
    /// Embedding the query, or finding it cached.
    pub embed_ms: u64,
    /// Every store leg, run concurrently: the slowest one's wall time.
    pub store_ms: u64,
    /// The scoped vector leg, connection wait included.
    pub semantic_ms: u64,
    /// The scoped vector leg's wait for a pooled connection.
    pub semantic_acquire_ms: u64,
    /// Each candidate pass of the scoped vector leg.
    pub semantic_pass_ms: Vec<u64>,
    /// The scoped vector leg's final candidate limit.
    pub candidates: i64,
    /// The scoped keyword leg, connection wait included.
    pub lexical_ms: u64,
    /// The scoped keyword leg's wait for a pooled connection.
    pub lexical_acquire_ms: u64,
    /// The unscoped vector leg behind the "other repositories" hint, when it ran.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wide_semantic_ms: Option<u64>,
    /// The unscoped keyword leg, when it ran.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wide_lexical_ms: Option<u64>,
    /// Candidate passes the unscoped vector leg took, when it ran.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wide_passes: Option<usize>,
    /// The unscoped legs were still running when this answer was ready, and
    /// it went without them: the hint is advice and never waits long.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub wide_dropped: bool,
    /// Fusing and balancing the two channels.
    pub rank_ms: u64,
    /// Attaching the agent that wrote each conversation hit.
    pub agents_ms: u64,
    /// Working out why a page is empty, when it is.
    pub diagnose_ms: u64,
}

/// Which repositories a search page was drawn from, as data.
///
/// A bare search is scoped to the repository the caller stands in, and saying
/// so only in `next_action` prose was not enough: on 2026-10-13 an analyst
/// agent spent half an hour believing recall was broken, finding 3 of 30
/// targets from the flowspace3 checkout against 29 of 30 from `/tmp`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchScope {
    /// The repository identity searched, or `None` for every repository.
    pub repo: Option<String>,
    /// How the scope was chosen: `cwd` (the caller's working directory, the
    /// default), `explicit` (`--repo`), or `all`.
    pub applied: String,
    /// Hits from OTHER repositories in the top page of the same query across
    /// every repository. `None` when that was not measured: only the first
    /// page of a `cwd` scope is.
    pub other_repo_hits: Option<usize>,
    /// An out-of-scope hit scored higher than this scope's best hit (or this
    /// scope found nothing and others did).
    pub outscored: bool,
}
