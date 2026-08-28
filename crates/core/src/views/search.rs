//! What `search` answers with.

use serde::{Deserialize, Serialize};

/// One hit, in the workshop-003 row shape.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
    /// The registered worktree root that supplied this hit.
    pub worktree: Option<String>,
}

/// What `GET /search` answers with.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SearchResults {
    /// Ranked hits, best first.
    pub results: Vec<Hit>,
}
