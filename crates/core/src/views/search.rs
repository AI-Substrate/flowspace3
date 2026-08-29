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
}

/// What `GET /search` answers with.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SearchResults {
    /// Ranked hits, best first.
    pub results: Vec<Hit>,
}
