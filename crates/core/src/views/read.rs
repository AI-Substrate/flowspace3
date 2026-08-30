//! What `get` and `tree` answer with.

use serde::{Deserialize, Serialize};

use crate::conversation::TurnItem;

/// One element, with everything the store knows about it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GetResult {
    /// The canonical address of what was returned.
    pub address: String,
    /// The repository it was read from, when a live path holds it.
    pub repo: Option<String>,
    /// The file it lives in, relative to its worktree root.
    pub path: String,
    /// The worktree root, so a caller can open the file on disk.
    pub root_path: Option<String>,
    /// The element's universal category.
    pub kind: String,
    /// The grammar's own kind, or the language for a whole file.
    pub subkind: String,
    /// The declaration's own name.
    pub name: String,
    /// Inclusive 1-based `[start, end]`.
    pub span: [u32; 2],
    /// The element's exact source — the whole file when the address named one.
    pub raw_text: String,
    /// The summary, when one has been made.
    pub smart: Option<String>,
    /// Concept tags from that summary (PRD req 36).
    pub tags: Vec<String>,
    /// The chain from the file down to this element, outermost first.
    pub parents: Vec<Outline>,
    /// What is declared inside it, to the requested depth.
    pub children: Vec<Outline>,
    /// Dirty element-tree shapes encountered while serving this result.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inconsistencies: Vec<super::status::ElementTreeInconsistency>,
}

/// What `get` answers with — an element, or a window of turns.
///
/// Untagged, so the envelope's `data` IS the payload rather than a wrapper a
/// consumer has to unwrap. The two shapes are told apart by their `address`
/// scheme, which is the discriminator workshop 003 already gave every caller,
/// so adding a tag would be a second one to keep in step.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum GetPayload {
    /// An `el:` address: one element, with its content and neighbours.
    Element(Box<GetResult>),
    /// A `conv:` address: a contiguous run of turns.
    Conversation(ConversationWindow),
}

/// A contiguous run of turns around one ordinal.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ConversationWindow {
    /// `conv:<guid>` — the conversation itself.
    pub address: String,
    /// The anchor repository identity, when the conversation has one.
    pub repo: Option<String>,
    /// The anchor checkout path.
    pub worktree: Option<String>,
    /// The commit the conversation started from.
    pub base_sha: Option<String>,
    /// The conversation's title, when it has one.
    pub title: Option<String>,
    /// How many turns the conversation holds in total.
    pub turns: i64,
    /// The ordinal the window is centred on.
    pub around: u32,
    /// The turns themselves, in order.
    pub window: Vec<TurnView>,
}

/// One turn, as `get` returns it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TurnView {
    /// `conv:<guid>#t<ord>` — addressable on its own.
    pub address: String,
    /// Position in the conversation.
    pub turn_no: u32,
    /// `human` or `agent`.
    pub role: String,
    /// `human`, `peer` or `system` — where the turn came from, which is not the
    /// same question as who wrote it (workshop 005, C8).
    pub source: String,
    /// Repo HEAD at time-of-turn, when there was one.
    pub head_sha: Option<String>,
    /// When it happened, RFC 3339 in UTC.
    pub at: String,
    /// The turn's prose, verbatim.
    pub body: String,
    /// Its typed sub-items, already shaped by the intake policy.
    pub items: Vec<TurnItem>,
}

/// A structural row: an address and enough to recognise it, with no content.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Outline {
    /// The child's address, in the same currency as everything else.
    pub address: String,
    /// Its universal category.
    pub kind: String,
    /// Its declared name.
    pub name: String,
    /// Inclusive 1-based `[start, end]`.
    pub span: [u32; 2],
    /// Its own children, when the requested depth reaches them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<Outline>,
}

/// What `tree` answered with.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TreeResult {
    /// What was actually browsed, as an address or path.
    pub target: String,
    /// The repository browsed, when the target named or implied one.
    pub repo: Option<String>,
    /// `index`, `repository`, `directory` or `file` — what the target turned
    /// out to be.
    pub kind: String,
    /// How many files exist under the target (or elements, for a file).
    pub total: i64,
    /// How many of them this answer lists.
    pub showing: usize,
    /// The structure itself.
    pub entries: Vec<TreeEntry>,
    /// Dirty element-tree shapes encountered while serving this result.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inconsistencies: Vec<super::status::ElementTreeInconsistency>,
}

/// One row of structure.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TreeEntry {
    /// `repository`, `directory`, `file`, or an element kind.
    pub kind: String,
    /// The segment's own name.
    pub name: String,
    /// The address to `get` or `tree` next, when the row has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    /// The repo-relative path, for files and directories.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Inclusive 1-based `[start, end]`, for elements.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span: Option<[u32; 2]>,
    /// How many files this row contains, for directories and repositories.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub files: Option<i64>,
    /// Who spoke, for a turn row (workshop 005's outline: role, source, time,
    /// first line). Absent on every code row, because a function has no role.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// Where the turn came from — `human`, `peer` or `system`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// When it happened, RFC 3339 in UTC.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<String>,
    /// Nested structure, to the requested depth.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<TreeEntry>,
}
