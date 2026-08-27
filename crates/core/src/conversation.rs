//! Conversations: turns as first-class, addressable content (req-0024..0027).
//!
//! Workshop 005 is the design. Two ideas carry this module:
//!
//! * A turn's **canonical stored form** ([`Turn::canonical`]) is a single
//!   readable text — the thing a summariser reads, the thing a raw vector
//!   embeds, and the thing whose hash is the turn's content address. One
//!   rendering, three jobs, so they cannot drift.
//! * That rendering carries **content only** — no timestamp, no role, no
//!   `head_sha`. Two agents that produced byte-identical text hash identically
//!   and therefore share one paid summary and one pair of vectors, which is the
//!   same dedupe the content layer already gives code (workshop 002 D2).
//!   Metadata lives in columns, where filtering wants it anyway.
//!
//! Nothing here talks to a database or a clock: timestamps arrive as text the
//! store formats server-side (the rule `fs3_store::messages` sets out), and the
//! store resolves the anchor identity to whatever rows it keeps.

use serde::{Deserialize, Serialize};

use crate::address::{CONVERSATION_SCHEME, TURN_SEPARATOR};
use crate::element::{Element, ElementKind, Span, content_hash};
use crate::error::{Error, Result};

/// The `parser_version` every turn element is written under.
///
/// Turn elements are rootless — there is no `kind = 'file'` node above them —
/// and `fs3_store::get_elements` refuses a `(blob_sha, parser_version)` pair
/// that does not have exactly one file root. Reserving a namespace the code
/// scanner never asks for is what keeps a canonical form that happens to hash
/// equal to a source file's blob from turning that file's next scan into a
/// corruption error. Bumping it re-mints turn elements without touching
/// enrichment, exactly as a parser bump does for code.
pub const PARSER_VERSION: &str = "conversation/1";

/// A conversation's identity: a UUID, as text.
///
/// Validated on construction so a malformed guid can never reach the store,
/// the same bargain [`crate::BlobRef`] makes. Text rather than a parsed UUID
/// because the store speaks to Postgres without sqlx's `uuid` feature — the
/// value is cast at the query edge and never re-formatted, so there is exactly
/// one spelling of a given conversation.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ConversationId(String);

impl ConversationId {
    /// The 8-4-4-4-12 group widths of a canonical UUID.
    const GROUPS: [usize; 5] = [8, 4, 4, 4, 12];

    /// Build a conversation id from canonical lowercase UUID text.
    ///
    /// # Errors
    /// [`Error::InvalidConversationId`] when the value is not 8-4-4-4-12
    /// lowercase hex.
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        let mut groups = value.split('-');
        let widths_match = Self::GROUPS.iter().all(|width| {
            groups.next().is_some_and(|group| {
                group.len() == *width
                    && group.bytes().all(|b| {
                        b.is_ascii_digit() || (b.is_ascii_lowercase() && b.is_ascii_hexdigit())
                    })
            })
        });

        if widths_match && groups.next().is_none() {
            Ok(ConversationId(value))
        } else {
            Err(Error::InvalidConversationId {
                value,
                reason: "not a canonical lowercase 8-4-4-4-12 uuid",
            })
        }
    }

    /// The guid as stored.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Workshop 003's address for the conversation itself: `conv:<guid>`.
    ///
    /// Spelled with [`crate::address`]'s own constants, which is the point:
    /// that module PARSES these addresses, and a renderer carrying its own
    /// copy of the scheme could drift from the parser with every test still
    /// green. One spelling, both directions.
    #[must_use]
    pub fn address(&self) -> String {
        let mut address = String::with_capacity(CONVERSATION_SCHEME.len() + self.0.len());
        address.push_str(CONVERSATION_SCHEME);
        address.push_str(&self.0);
        address
    }

    /// Workshop 003's address for one turn: `conv:<guid>#t<ord>`.
    #[must_use]
    pub fn turn_address(&self, turn_no: u32) -> String {
        let mut address = self.address();
        address.push_str(TURN_SEPARATOR);
        address.push_str(itoa(turn_no).as_str());
        address
    }
}

impl std::fmt::Display for ConversationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for ConversationId {
    type Error = Error;
    fn try_from(value: String) -> Result<Self> {
        ConversationId::new(value)
    }
}

impl From<ConversationId> for String {
    fn from(value: ConversationId) -> Self {
        value.0
    }
}

/// Who is speaking.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnRole {
    /// A person.
    Human,
    /// A model.
    Agent,
}

/// Where the turn came FROM, which is not the same question as who wrote it.
///
/// Measured (workshop 005, C8): in an orchestrated fleet, peer-injected turns
/// equal human turns in count. A `role`-only model would report an agent fleet
/// as half-human.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnSource {
    /// Typed by a person.
    Human,
    /// Injected by another agent (a pij send, an orchestrator packet).
    Peer,
    /// Emitted by the harness itself.
    System,
}

/// What a tool was asked to do.
///
/// Verbatim, except for the write/edit family: measured at 1.2MB of a 2.85MB
/// input side, and the body is the very next commit, so storing it here doubles
/// the input bill for zero search value (workshop 005, C3).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ToolInput {
    /// The call's arguments as they were made.
    Verbatim {
        /// The argument text.
        text: String,
    },
    /// A write-family body, replaced by what it addressed.
    Elided {
        /// The path that was written.
        path: String,
        /// How many bytes the body was.
        bytes: u64,
    },
}

/// A typed sub-item of a turn (req-0025).
///
/// JSONB in the store, so a new kind is a code change and never a migration.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TurnItem {
    /// A tool invocation.
    ToolCall {
        /// The tool's name, as the harness spells it.
        tool: String,
        /// What it was asked to do.
        input: ToolInput,
    },
    /// A tool's answer, already cut to its head at intake.
    ///
    /// The head keeps 62.7% of results whole and the opening lines of every
    /// error — errors front-load — at 35.6% of the output bytes (C2).
    ToolResult {
        /// The tool's name, as the harness spells it.
        tool: String,
        /// The first bytes of the result.
        head: String,
        /// How large the whole result was.
        total_bytes: u64,
        /// Whether `head` is short of `total_bytes`.
        truncated: bool,
    },
}

/// One turn: the prose, its typed sub-items, and where in the sequence it sits.
///
/// `turn_no` is dense from 1 and IS the navigation axis — sequence is to a
/// conversation what hierarchy is to code (req-0026).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Turn {
    /// Position in the conversation, dense from 1.
    pub turn_no: u32,
    /// Who is speaking.
    pub role: TurnRole,
    /// Where the turn came from.
    pub source: TurnSource,
    /// Repo HEAD at time-of-turn, when there was one.
    ///
    /// Truncated tool output is only honest if the state it came from is
    /// addressable (C6): this is that address.
    pub head_sha: Option<String>,
    /// When the turn happened, RFC 3339 in UTC.
    ///
    /// Text, not a parsed instant: core owns no clock and the store formats
    /// timestamps server-side, so two machines cannot disagree about "now".
    pub at: String,
    /// The turn's prose, verbatim.
    pub body: String,
    /// Typed sub-items, in the order they happened.
    pub items: Vec<TurnItem>,
}

impl Turn {
    /// The turn's canonical stored form — the ONE text that is summarised,
    /// embedded, hashed and displayed.
    ///
    /// Content only. Timestamps, roles and shas are deliberately absent: two
    /// byte-identical turns in two different conversations must hash the same
    /// so they share one paid enrichment, and metadata that varies per
    /// occurrence would defeat exactly that.
    #[must_use]
    pub fn canonical(&self) -> String {
        let mut out = String::with_capacity(self.canonical_capacity());
        self.render_canonical(&mut out);
        out
    }

    /// The content address of [`Turn::canonical`] — the bridge into the
    /// element/content layer, and the value stored as `turns.blob_sha`.
    #[must_use]
    pub fn blob_sha(&self) -> String {
        content_hash(self.canonical().as_bytes())
    }

    /// This turn as an addressable element, ready for the content layer.
    ///
    /// `span` is the ordinal on both ends: a turn occupies one position in the
    /// sequence, and the sequence is the axis `get --before/--after` walks. The
    /// `elements_span_ordered` check is satisfied for free because a dense
    /// `turn_no` starts at 1.
    #[must_use]
    pub fn element(&self, conversation: &ConversationId) -> Element {
        Element::new(
            ElementKind::Turn,
            self.role.as_str(),
            self.name(),
            conversation.turn_address(self.turn_no),
            Span::new(self.turn_no, self.turn_no),
            self.canonical(),
        )
    }

    /// The turn's short name, `t<ord>` — what an outline row is labelled with.
    #[must_use]
    pub fn name(&self) -> String {
        let ordinal = itoa(self.turn_no);
        let mut name = String::with_capacity(1 + ordinal.as_str().len());
        name.push('t');
        name.push_str(ordinal.as_str());
        name
    }

    /// Enough room for the whole rendering, so it is built in one allocation.
    fn canonical_capacity(&self) -> usize {
        // 48 bytes per item covers the marker line for every shape below.
        const ITEM_OVERHEAD: usize = 48;
        self.body.len()
            + self
                .items
                .iter()
                .map(|item| ITEM_OVERHEAD + item.rendered_len())
                .sum::<usize>()
    }

    /// Body first, then one block per item, blocks separated by a blank line.
    fn render_canonical(&self, out: &mut String) {
        let body = self.body.trim_end();
        out.push_str(body);
        for item in &self.items {
            if !out.is_empty() {
                out.push_str("\n\n");
            }
            item.render(out);
        }
    }
}

impl TurnItem {
    /// The bytes this item contributes, marker text excluded.
    fn rendered_len(&self) -> usize {
        match self {
            TurnItem::ToolCall { tool, input } => {
                tool.len()
                    + match input {
                        ToolInput::Verbatim { text } => text.len(),
                        ToolInput::Elided { path, .. } => path.len(),
                    }
            }
            TurnItem::ToolResult { tool, head, .. } => tool.len() + head.len(),
        }
    }

    /// Render one block of the canonical form.
    ///
    /// Marker lines are prose rather than a parseable frame on purpose: this
    /// text is never read back — the typed value in `items` is the record, and
    /// this is what a model reads. Two structurally different turns that render
    /// identically are, for every purpose this text serves, the same content.
    fn render(&self, out: &mut String) {
        match self {
            TurnItem::ToolCall { tool, input } => {
                out.push_str("[tool-call ");
                out.push_str(tool);
                match input {
                    ToolInput::Verbatim { text } => {
                        out.push_str("]\n");
                        out.push_str(text);
                    }
                    ToolInput::Elided { path, bytes } => {
                        out.push_str("] ");
                        out.push_str(path);
                        out.push_str(" (");
                        out.push_str(itoa64(*bytes).as_str());
                        out.push_str(" bytes, body elided)");
                    }
                }
            }
            TurnItem::ToolResult {
                tool,
                head,
                total_bytes,
                truncated,
            } => {
                out.push_str("[tool-result ");
                out.push_str(tool);
                out.push_str("] ");
                out.push_str(itoa64(*total_bytes).as_str());
                out.push_str(if *truncated {
                    " bytes, truncated\n"
                } else {
                    " bytes\n"
                });
                out.push_str(head);
            }
        }
    }
}

/// A conversation header: identity, anchor, and when it began.
///
/// The anchor is a POINTER, not ownership (workshop 005 OQ2, ruled 2026-08-27):
/// `repo_identity` is `repos.identity` as text with no foreign key, so removing
/// the last worktree of a repository leaves its conversations intact and
/// re-adding that repository re-links them for free.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Conversation {
    /// Caller-supplied or minted at import.
    pub guid: ConversationId,
    /// Anchor: the repository identity this conversation happened in.
    pub repo_identity: Option<String>,
    /// Anchor: the checkout path, within or beside the repository.
    pub worktree: Option<String>,
    /// Anchor: the commit the conversation started from.
    pub base_sha: Option<String>,
    /// Optional; import may derive it from the first turn.
    pub title: Option<String>,
    /// When the conversation began, RFC 3339 in UTC.
    pub started_at: String,
}

/// Whether a turn's stored form earns its own LLM summary (workshop 005).
///
/// A byte floor, not the line floor code uses ([`crate::needs_summary`]): a
/// five-word turn does not earn an LLM call and raw is its own display form,
/// while a one-line turn carrying a 4KB tool result plainly does. Turns are
/// one span-line each, so a line floor could not tell those apart at all.
#[must_use]
pub const fn earns_summary(canonical: &str, min_bytes: usize) -> bool {
    canonical.len() >= min_bytes
}

impl TurnRole {
    /// The stable wire/storage spelling. Matches the serde representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            TurnRole::Human => "human",
            TurnRole::Agent => "agent",
        }
    }
}

impl std::str::FromStr for TurnRole {
    type Err = Error;

    /// Read a stored spelling back.
    ///
    /// # Errors
    /// [`Error::InvalidConfig`] naming the value, for a row that is not one of
    /// the two the column's check constraint allows.
    fn from_str(value: &str) -> Result<Self> {
        match value {
            "human" => Ok(TurnRole::Human),
            "agent" => Ok(TurnRole::Agent),
            other => Err(Error::InvalidConfig(format!("unknown turn role {other:?}"))),
        }
    }
}

impl std::fmt::Display for TurnRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl TurnSource {
    /// The stable wire/storage spelling. Matches the serde representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            TurnSource::Human => "human",
            TurnSource::Peer => "peer",
            TurnSource::System => "system",
        }
    }
}

impl std::str::FromStr for TurnSource {
    type Err = Error;

    /// Read a stored spelling back.
    ///
    /// # Errors
    /// [`Error::InvalidConfig`] naming the value, for a row that is not one of
    /// the three the column's check constraint allows.
    fn from_str(value: &str) -> Result<Self> {
        match value {
            "human" => Ok(TurnSource::Human),
            "peer" => Ok(TurnSource::Peer),
            "system" => Ok(TurnSource::System),
            other => Err(Error::InvalidConfig(format!(
                "unknown turn source {other:?}"
            ))),
        }
    }
}

impl std::fmt::Display for TurnSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A decimal rendering with no allocation.
///
/// Addresses are minted per turn on every read path, and `format!` for a
/// three-digit ordinal is a heap allocation to build a string that is about to
/// be pushed into another one.
struct Digits {
    buffer: [u8; 20],
    start: usize,
}

impl Digits {
    fn as_str(&self) -> &str {
        // Every byte written is an ASCII digit.
        core::str::from_utf8(&self.buffer[self.start..]).unwrap_or("0")
    }
}

fn itoa(value: u32) -> Digits {
    itoa64(u64::from(value))
}

fn itoa64(mut value: u64) -> Digits {
    let mut digits = Digits {
        buffer: [0; 20],
        start: 20,
    };
    loop {
        digits.start -= 1;
        digits.buffer[digits.start] = b'0' + (value % 10) as u8;
        value /= 10;
        if value == 0 {
            return digits;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn id() -> ConversationId {
        ConversationId::new("6ba7b810-9dad-11d1-80b4-00c04fd430c8").unwrap()
    }

    fn turn(turn_no: u32, body: &str) -> Turn {
        Turn {
            turn_no,
            role: TurnRole::Agent,
            source: TurnSource::System,
            head_sha: None,
            at: "2026-08-27T01:02:03Z".to_string(),
            body: body.to_string(),
            items: Vec::new(),
        }
    }

    #[test]
    fn conversation_id_accepts_canonical_uuid_text_only() {
        assert!(ConversationId::new("6ba7b810-9dad-11d1-80b4-00c04fd430c8").is_ok());
        // Uppercase is a second spelling of one conversation; refuse it.
        assert!(ConversationId::new("6BA7B810-9DAD-11D1-80B4-00C04FD430C8").is_err());
        assert!(ConversationId::new("6ba7b810-9dad-11d1-80b4").is_err());
        assert!(ConversationId::new("6ba7b810-9dad-11d1-80b4-00c04fd430c8-extra").is_err());
        assert!(ConversationId::new("not-a-uuid").is_err());
    }

    #[test]
    fn addresses_follow_workshop_003() {
        assert_eq!(id().address(), "conv:6ba7b810-9dad-11d1-80b4-00c04fd430c8");
        assert_eq!(
            id().turn_address(42),
            "conv:6ba7b810-9dad-11d1-80b4-00c04fd430c8#t42"
        );
    }

    /// The invariant the literals above cannot carry on their own: what this
    /// module RENDERS is what [`crate::address`] PARSES.
    ///
    /// Before the sweep the two modules held separate copies of `conv:` and
    /// `#t`, so a change to the scheme in one of them would have left the
    /// renderer and the parser disagreeing with every test still green — the
    /// literals above would have been "corrected" alongside the renderer and
    /// gone on passing. This asserts the round trip instead of the spelling,
    /// so it fails on divergence rather than on rewording.
    #[test]
    fn a_rendered_address_parses_back_to_the_same_conversation_and_turn() {
        let id = id();

        let Ok(crate::Address::Conversation(whole)) = crate::Address::parse(&id.address()) else {
            panic!("a rendered conversation address must parse as one");
        };
        assert_eq!(whole.guid, id.as_str());
        assert_eq!(whole.turn, None);
        assert_eq!(whole.to_string(), id.address());

        let Ok(crate::Address::Conversation(turn)) = crate::Address::parse(&id.turn_address(42))
        else {
            panic!("a rendered turn address must parse as one");
        };
        assert_eq!(turn.guid, id.as_str());
        assert_eq!(turn.turn, Some(42));
        assert_eq!(turn.to_string(), id.turn_address(42));
    }

    /// The dedupe property the whole spend story rests on: same content, same
    /// hash, whatever conversation it happened in and whenever it happened.
    #[test]
    fn identical_content_hashes_identically_across_conversations() {
        let mut first = turn(1, "the same words");
        let mut second = turn(97, "the same words");
        second.role = TurnRole::Human;
        second.source = TurnSource::Peer;
        second.at = "2019-01-01T00:00:00Z".to_string();
        second.head_sha = Some("deadbeefdeadbeefdeadbeefdeadbeefdeadbeef".to_string());

        assert_eq!(first.blob_sha(), second.blob_sha());

        // And it is the hash of the canonical form, not of something else.
        assert_eq!(
            first.blob_sha(),
            content_hash(first.canonical().as_bytes()),
            "blob_sha must BE the content address of the canonical form"
        );

        // Different content, different hash — the property is not vacuous.
        first.body = "different words".to_string();
        assert_ne!(first.blob_sha(), second.blob_sha());
    }

    #[test]
    fn canonical_form_renders_body_then_items() {
        let mut subject = turn(3, "ran the gate\n\n");
        subject.items = vec![
            TurnItem::ToolCall {
                tool: "bash".to_string(),
                input: ToolInput::Verbatim {
                    text: "cargo test --all".to_string(),
                },
            },
            TurnItem::ToolCall {
                tool: "write".to_string(),
                input: ToolInput::Elided {
                    path: "src/lib.rs".to_string(),
                    bytes: 2048,
                },
            },
            TurnItem::ToolResult {
                tool: "bash".to_string(),
                head: "running 4 tests".to_string(),
                total_bytes: 91_233,
                truncated: true,
            },
        ];

        assert_eq!(
            subject.canonical(),
            "ran the gate\n\n\
             [tool-call bash]\ncargo test --all\n\n\
             [tool-call write] src/lib.rs (2048 bytes, body elided)\n\n\
             [tool-result bash] 91233 bytes, truncated\nrunning 4 tests"
        );
    }

    /// A turn with no prose is common (a bare tool call) and must not open with
    /// a blank line — the leading separator would change the hash of content
    /// that is otherwise identical to the same call made with prose stripped.
    #[test]
    fn a_bodyless_turn_starts_at_its_first_item() {
        let mut subject = turn(1, "");
        subject.items = vec![TurnItem::ToolCall {
            tool: "read".to_string(),
            input: ToolInput::Verbatim {
                text: "AGENTS.md".to_string(),
            },
        }];

        assert_eq!(subject.canonical(), "[tool-call read]\nAGENTS.md");
    }

    #[test]
    fn element_is_a_turn_kind_at_its_ordinal() {
        let subject = turn(7, "hello");
        let element = subject.element(&id());

        assert_eq!(element.kind, ElementKind::Turn);
        assert_eq!(element.subkind, "agent");
        assert_eq!(element.name, "t7");
        assert_eq!(
            element.address,
            "conv:6ba7b810-9dad-11d1-80b4-00c04fd430c8#t7"
        );
        assert_eq!(element.span, Span::new(7, 7));
        assert_eq!(element.raw_text, "hello");
        // The bridge: the element's dirtiness key IS the turn's content address.
        assert_eq!(element.raw_hash(), subject.blob_sha());
    }

    #[test]
    fn summary_gate_is_a_byte_floor() {
        assert!(!earns_summary("tiny", 256));
        assert!(earns_summary(&"x".repeat(256), 256));
    }

    #[test]
    fn roles_and_sources_round_trip_through_their_stored_spellings() {
        for role in [TurnRole::Human, TurnRole::Agent] {
            assert_eq!(TurnRole::from_str(role.as_str()).unwrap(), role);
        }
        for source in [TurnSource::Human, TurnSource::Peer, TurnSource::System] {
            assert_eq!(TurnSource::from_str(source.as_str()).unwrap(), source);
        }
        assert!(TurnRole::from_str("peer").is_err());
        assert!(TurnSource::from_str("agent").is_err());
    }

    #[test]
    fn items_survive_a_json_round_trip_with_their_kinds() {
        let items = vec![
            TurnItem::ToolCall {
                tool: "edit".to_string(),
                input: ToolInput::Elided {
                    path: "a.rs".to_string(),
                    bytes: 12,
                },
            },
            TurnItem::ToolResult {
                tool: "edit".to_string(),
                head: "ok".to_string(),
                total_bytes: 2,
                truncated: false,
            },
        ];
        let json = serde_json::to_string(&items).unwrap();
        assert_eq!(
            serde_json::from_str::<Vec<TurnItem>>(&json).unwrap(),
            items,
            "items are stored as JSONB and read back as typed values"
        );
    }
}
