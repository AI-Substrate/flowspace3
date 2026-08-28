//! Claude Code's native session store: session jsonl, subagent sidecars, and
//! spilled tool results (plan 005, unit u1a).
//!
//! # The dialect, and only the dialect
//!
//! Framing — the tail buffer, the byte-offset cursor, rotation and truncation
//! detection, the torn-line rule — is [`super::tail::read_lines`] and is not
//! repeated here. What is left is what makes Claude Code's store different
//! from the other three, and it is genuinely different in four ways.
//!
//! ## 1. One line per content BLOCK, not per message
//!
//! Claude writes a separate jsonl record for every content block a message
//! contains, and every one of them repeats the same `message.id`. Measured on
//! the committed fixture: session `a5a5588f` holds 38 `assistant` records over
//! 13 distinct `message.id` values, and `b1d6f4fb` holds 6 over 2. A reader
//! that emits one turn per record reports 38 assistant turns where a human
//! reading the transcript sees 13.
//!
//! The blocks of one message are NOT adjacent in the file. A tool-use loop
//! writes `assistant`(tool_use) → `user`(tool_result) → `assistant`(same
//! `message.id`), so collapsing adjacent runs yields 20 groups on that fixture
//! rather than 13. Grouping is therefore KEYED by `message.id` over the
//! assistant projection — [`merge_records`] — and the fixture is built so that
//! an adjacent-run fold fails the test rather than looking plausible.
//!
//! What makes that safe to do in one pass: no `message.id` ever reappears
//! after a different one has intervened. The assistant projection is grouped;
//! it is only the interleaved `user` records that break adjacency in the raw
//! file. A merged turn is emitted at the position of its FIRST block, so the
//! ordinals a batch yields stay in the store's own order.
//!
//! ## 2. The ordinal of a merged turn is its FIRST block's uuid
//!
//! [`RawRecord::ordinal`] is the dedupe key, and `fs3_testkit::Expectations`
//! holds every reader to emitting an in-order, repeat-free SUBSEQUENCE of the
//! ids the store actually wrote — which are the per-line `uuid` values, not
//! `message.id`. So a merged turn reports the first uuid of its group and
//! silently does not emit the rest, which is exactly the "fewer records than
//! the store holds" the expectations licence.
//!
//! First rather than last, deliberately: it is stable under a rescan. A full
//! re-read regroups the same blocks and computes the same first uuid, so the
//! dedupe ledger recognises the record it already stored. The last uuid of a
//! group changes as the group grows and would defeat that.
//!
//! ## 3. A group split across two polls yields two turns, permanently
//!
//! A live session can be polled mid-message: blocks 1-2 land in poll N and
//! block 3 in poll N+1. This reader does NOT hold back the trailing group.
//! Holding it back would mean a session that ends mid-message never emits its
//! final turn at all — silent loss, on exactly the conversation someone is
//! watching live.
//!
//! The consequence is worth stating exactly, because it is permanent and not
//! merely a delay. Poll N stores blocks 1-2 under `uuid(b1)`. Poll N+1 stores
//! block 3 under `uuid(b3)`. A later rotation forces a rescan, which regroups
//! all three blocks and emits ONE record under `uuid(b1)` — which the dedupe
//! ledger has already seen, so it is dropped. The turn stored under `uuid(b1)`
//! therefore keeps blocks 1-2 FOREVER and is never backfilled. Nothing is
//! lost and nothing duplicates: one assistant message simply reads as two
//! turns. Accepted for v1 (PM ruling, plan 005 wave 1).
//!
//! ## 4. Oversized tool results are spilled to a sibling file
//!
//! A large tool result is written to `<session>/tool-results/<name>` and the
//! record keeps only a ~2KB preview under `toolUseResult.stdout`. The record
//! also carries `persistedOutputPath` — an ABSOLUTE path from the machine that
//! wrote it (`/Users/agent/.claude/projects/...` in the committed fixture),
//! which does not exist on the machine reading it. Only its FILE NAME is
//! portable, so the spill is resolved as
//! `<session dir>/tool-results/<file name>` and the absolute path is never
//! opened. An unresolvable spill falls back to the preview marked
//! `truncated` — a tool result that cannot be read is not a reason to fail an
//! entire ingest.
//!
//! # This reader is LOSSLESS; payload policy belongs to the normaliser
//!
//! The v1 payload policy — head-truncating tool results to 512 B, eliding
//! write-family tool bodies to a path plus a length, dropping `thinking`
//! blocks — is the NORMALISER's job, per the plan-005 impl-guide: what is left
//! for it is to "apply the payload policy, drop what v1 does not store". So
//! everything here is verbatim: `thinking` and `text` blocks both reach
//! [`RawRecord::body`], tool inputs are whole, and a resolved spill is its full
//! bytes with `truncated: false`. A reader that applied the policy itself
//! would destroy data the policy might later want back, and a policy applied
//! twice in two crates is a policy that will drift.
//!
//! Note for whoever owns that policy: `thinking` blocks are NOT absent from
//! claude data, whatever the payload spec's parenthetical says. The committed
//! fixture holds 21 of them against 5 `text` blocks, so dropping them is a real
//! content decision about the majority of assistant prose, not a no-op.
//!
//! # The record-type allowlist is a BEHAVIOUR, not an enumeration
//!
//! Claude's store interleaves bookkeeping rows with conversation: the
//! committed fixtures alone hold 14 distinct record types. Only `user` and
//! `assistant` bear turns. Everything else — `attachment`, `last-prompt`,
//! `custom-title`, `ai-title`, `agent-name`, `mode`, `permission-mode`,
//! `atis-latch`, `pr-link`, `file-history-delta`, `file-history-snapshot`,
//! `queue-operation` — is store metadata that describes the session rather
//! than anything said in it, and is dropped.
//!
//! Crucially the rule is stated as "everything that is not turn-bearing is
//! dropped", never as a list to match: an UNKNOWN record type is a drop, not
//! an error and not a panic. Anthropic will add a 15th type, and an ingest
//! that fails because a store grew a bookkeeping row is a worse outcome than
//! one that ignores it (PM ruling, plan 005 wave 1).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use fs3_core::{
    ConversationSource, Error, Harness, IngestInput, RawRecord, ReadBatch, Result, SessionFile,
    SessionKind, SourceCursor, ToolInput, TurnItem, TurnRole, TurnSource,
};
use serde::Deserialize;

use super::tail;

/// Reads conversations out of a Claude Code project directory.
///
/// `root` is the workspace-slugged directory that holds the session files —
/// `~/.claude/projects/<slug>` in production, a scratch copy in tests. The
/// reader never derives the slug itself: resolution from a workspace folder to
/// a slug is the orchestrator's job, and keeping it out of here is what makes
/// the reader testable against a directory that is not under `$HOME`.
#[derive(Clone, Debug)]
pub struct ClaudeSource {
    root: PathBuf,
}

impl ClaudeSource {
    /// A reader over one Claude Code project directory.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The session id this input names, refusing anything this store cannot
    /// answer.
    fn session_id<'a>(&self, input: &'a IngestInput) -> Result<&'a str> {
        match input {
            IngestInput::Native {
                session_id,
                harness: Harness::Claude,
                ..
            } => Ok(session_id),
            IngestInput::Native { harness, .. } => Err(Error::Provider(format!(
                "claude: asked for a `{}` session; the claude reader holds only claude sessions",
                harness.as_str()
            ))),
            // The seat-to-session join is the orchestrator's, and doing it here
            // would put a pij dependency inside a claude dialect.
            IngestInput::Pij { id, .. } => Err(Error::Provider(format!(
                "claude: seat `{id}` must be resolved to a native session id before this reader \
                 is called"
            ))),
        }
    }
}

impl ConversationSource for ClaudeSource {
    fn harness(&self) -> Harness {
        Harness::Claude
    }

    fn resolve(&self, input: &IngestInput) -> Result<Vec<SessionFile>> {
        let session_id = self.session_id(input)?;
        let main = self.root.join(format!("{session_id}.jsonl"));
        if !main.is_file() {
            return Err(Error::Provider(format!(
                "claude: no session file at {}",
                main.display()
            )));
        }

        let mut files = vec![SessionFile {
            path: main,
            session_id: session_id.to_owned(),
            parent_session_id: None,
            kind: SessionKind::Main,
            harness: Harness::Claude,
        }];

        // Re-globbed on EVERY call, never cached: a subagent spawned after
        // ingestion began is a child conversation that must still be found.
        files.extend(self.sidecars(session_id)?);
        Ok(files)
    }

    fn read_incremental(
        &self,
        file: &SessionFile,
        cursor: Option<&SourceCursor>,
    ) -> Result<ReadBatch> {
        // Framing, rotation, truncation and the torn-line rule are all
        // tail's; a foreign cursor is refused there too.
        let read = tail::read_lines(&file.path, cursor)?;
        let lines = parse_lines(&read.lines);
        let tools = tool_names(&lines);
        let records = merge_records(&lines, &tools, &session_dir(file));

        Ok(ReadBatch {
            records,
            cursor: read.cursor,
            rescanned: read.rescanned,
        })
    }
}

impl ClaudeSource {
    /// Every subagent sidecar of a session, in a stable order.
    ///
    /// A missing `subagents/` directory is the normal case — most sessions
    /// spawn no subagent — so it yields nothing rather than an error.
    fn sidecars(&self, session_id: &str) -> Result<Vec<SessionFile>> {
        let directory = self.root.join(session_id).join("subagents");
        let entries = match std::fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => {
                return Err(Error::Provider(format!(
                    "claude: cannot list {}: {error}",
                    directory.display()
                )));
            }
        };

        let mut sidecars = Vec::new();
        for entry in entries {
            let path = entry
                .map_err(|error| {
                    Error::Provider(format!(
                        "claude: cannot read an entry of {}: {error}",
                        directory.display()
                    ))
                })?
                .path();
            if path
                .extension()
                .is_some_and(|extension| extension == "jsonl")
            {
                let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
                    continue;
                };
                sidecars.push(SessionFile {
                    session_id: stem.to_owned(),
                    // The sidecar's own `.meta.json` carries `agentType`,
                    // `description`, `toolUseId` and `spawnDepth` — but NOT the
                    // parent session id. The directory it sits in is the only
                    // place that link exists, which is where the next person to
                    // look for it will not think to look.
                    parent_session_id: Some(session_id.to_owned()),
                    kind: SessionKind::Subagent,
                    harness: Harness::Claude,
                    path,
                });
            }
        }
        // Directory order is not defined; the contract's file count is, and a
        // stable order keeps a batch reproducible.
        sidecars.sort_by(|left, right| left.path.cmp(&right.path));
        Ok(sidecars)
    }
}

/// The directory a session's spilled tool results live under.
///
/// `<root>/<id>.jsonl` keeps them in `<root>/<id>/tool-results`; a sidecar at
/// `<root>/<id>/subagents/<agent>.jsonl` shares its parent session's.
fn session_dir(file: &SessionFile) -> PathBuf {
    match file.kind {
        SessionKind::Main => file.path.with_extension(""),
        SessionKind::Subagent => file
            .path
            .parent()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .unwrap_or_default(),
    }
}

/// One jsonl line, reduced to the fields a turn is built from.
///
/// Every field is optional because this store's rows are heterogeneous: an
/// unparseable or unfamiliar line must be skipped, never fatal.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Line {
    #[serde(rename = "type")]
    record_type: String,
    uuid: Option<String>,
    parent_uuid: Option<String>,
    timestamp: Option<String>,
    message: Option<Message>,
    tool_use_result: Option<ToolUseResult>,
    #[serde(default)]
    is_meta: bool,
    #[serde(default)]
    is_compact_summary: bool,
    origin: Option<Origin>,
}

#[derive(Debug, Deserialize)]
struct Origin {
    kind: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Message {
    id: Option<String>,
    content: Option<Content>,
}

/// `message.content` is a bare string for a typed user turn and a block array
/// everywhere else.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Content {
    Text(String),
    Blocks(Vec<Block>),
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Block {
    Text {
        #[serde(default)]
        text: String,
    },
    Thinking {
        #[serde(default)]
        thinking: String,
    },
    ToolUse {
        #[serde(default)]
        id: String,
        #[serde(default)]
        name: String,
        input: Option<serde_json::Value>,
    },
    ToolResult {
        #[serde(default)]
        tool_use_id: String,
        content: Option<Content>,
    },
    /// An image, or whatever Anthropic adds next. Carries no prose this reader
    /// can store, and is not a reason to fail.
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ToolUseResult {
    persisted_output_path: Option<String>,
    persisted_output_size: Option<u64>,
    stdout: Option<String>,
}

/// Parse what parses, drop what does not.
///
/// A line this reader cannot understand is a line the store grew without
/// telling us, and skipping it keeps an ingest alive; failing on it would mean
/// one new bookkeeping row could stop every conversation on the machine.
fn parse_lines(lines: &[String]) -> Vec<Line> {
    lines
        .iter()
        .filter_map(|line| serde_json::from_str::<Line>(line).ok())
        .filter(|line| line.uuid.is_some())
        .collect()
}

/// `tool_use_id` → tool name, for the results that arrive without one.
///
/// A `tool_result` record names only the id of the call it answers, so the
/// name is recovered from the `tool_use` block that made it. That block is in
/// the same batch in the overwhelming majority of cases — a call and its
/// result are adjacent — but not when a poll lands between them, so callers
/// fall back to the id itself rather than inventing a name.
fn tool_names(lines: &[Line]) -> HashMap<&str, &str> {
    let mut names = HashMap::new();
    for line in lines {
        let Some(Content::Blocks(blocks)) = line.message.as_ref().and_then(|m| m.content.as_ref())
        else {
            continue;
        };
        for block in blocks {
            if let Block::ToolUse { id, name, .. } = block
                && !id.is_empty()
                && !name.is_empty()
            {
                names.insert(id.as_str(), name.as_str());
            }
        }
    }
    names
}

/// Turn parsed lines into records, merging assistant blocks by `message.id`.
///
/// The merge is KEYED, not an adjacent-run fold: a message's blocks are
/// routinely interrupted by the `user` tool_result records of its own tool
/// calls. Each merged turn is emitted at the index of its first block, so the
/// batch stays in store order and its ordinals stay a subsequence of the
/// store's own.
fn merge_records(
    lines: &[Line],
    tools: &HashMap<&str, &str>,
    session_dir: &Path,
) -> Vec<RawRecord> {
    // Position of each open assistant group in `out`, keyed by message id.
    let mut open: HashMap<&str, usize> = HashMap::new();
    let mut out: Vec<RawRecord> = Vec::new();

    for line in lines {
        match line.record_type.as_str() {
            "assistant" => {
                let (body, items) = blocks_of(line, tools, session_dir);
                match line.message.as_ref().and_then(|m| m.id.as_deref()) {
                    Some(id) => match open.get(id) {
                        // A later block of a message already begun: fold it in,
                        // leaving the group where its FIRST block put it.
                        Some(&at) => extend(&mut out[at], body, items),
                        None => {
                            open.insert(id, out.len());
                            out.push(record(
                                line,
                                TurnRole::Agent,
                                TurnSource::System,
                                body,
                                items,
                            ));
                        }
                    },
                    // No message id to group by: its own turn, which is the
                    // honest reading of a record that claims no message.
                    None => out.push(record(
                        line,
                        TurnRole::Agent,
                        TurnSource::System,
                        body,
                        items,
                    )),
                }
            }
            "user" => {
                let (body, items) = blocks_of(line, tools, session_dir);
                if body.is_empty() && items.is_empty() {
                    continue;
                }
                let source = user_source(line, &items);
                out.push(record(line, TurnRole::Human, source, body, items));
            }
            // Every other type is store bookkeeping. Unknown types land here
            // too, by construction.
            _ => {}
        }
    }

    out
}

/// Where a user record came from.
///
/// `origin.kind` is the store's own answer where it has one. Otherwise: a
/// record carrying tool results, a compaction summary or a harness meta row is
/// the harness talking, and anything left is a person typing.
///
/// Claude's store has no signal for a peer-injected turn — a `pij send` and a
/// typed message are byte-identical here — so [`TurnSource::Peer`] is only ever
/// reported when `origin.kind` says so. Guessing from message text would be
/// dialect invention, not dialect reading.
fn user_source(line: &Line, items: &[TurnItem]) -> TurnSource {
    match line
        .origin
        .as_ref()
        .and_then(|origin| origin.kind.as_deref())
    {
        Some("human") => TurnSource::Human,
        Some(_) => TurnSource::Peer,
        None => {
            if line.is_meta
                || line.is_compact_summary
                || items
                    .iter()
                    .any(|item| matches!(item, TurnItem::ToolResult { .. }))
            {
                TurnSource::System
            } else {
                TurnSource::Human
            }
        }
    }
}

/// The prose and the typed items one line contributes.
fn blocks_of(
    line: &Line,
    tools: &HashMap<&str, &str>,
    session_dir: &Path,
) -> (String, Vec<TurnItem>) {
    let mut body = String::new();
    let mut items = Vec::new();

    let Some(content) = line.message.as_ref().and_then(|m| m.content.as_ref()) else {
        return (body, items);
    };

    match content {
        Content::Text(text) => push_prose(&mut body, text),
        Content::Blocks(blocks) => {
            for block in blocks {
                match block {
                    Block::Text { text } => push_prose(&mut body, text),
                    // Verbatim: dropping thinking is the normaliser's policy
                    // call, and this reader does not pre-empt it.
                    Block::Thinking { thinking } => push_prose(&mut body, thinking),
                    Block::ToolUse { name, input, .. } => {
                        items.push(TurnItem::ToolCall {
                            tool: name.clone(),
                            // Whole: the write-family elision is payload policy.
                            input: ToolInput::Verbatim {
                                text: input.as_ref().map(ToString::to_string).unwrap_or_default(),
                            },
                        });
                    }
                    Block::ToolResult {
                        tool_use_id,
                        content,
                    } => items.push(tool_result(
                        tool_use_id,
                        content.as_ref(),
                        line.tool_use_result.as_ref(),
                        tools,
                        session_dir,
                    )),
                    Block::Other => {}
                }
            }
        }
    }

    (body, items)
}

/// One tool result, with its spilled body resolved where there is one.
fn tool_result(
    tool_use_id: &str,
    content: Option<&Content>,
    spill: Option<&ToolUseResult>,
    tools: &HashMap<&str, &str>,
    session_dir: &Path,
) -> TurnItem {
    let tool = tools
        .get(tool_use_id)
        .map_or_else(|| tool_use_id.to_owned(), |name| (*name).to_owned());

    let inline = content.map(flatten).unwrap_or_default();

    // The spill's absolute path belongs to the machine that wrote it; only the
    // file name survives the trip, so it is re-anchored under this session.
    let spilled = spill
        .and_then(|result| result.persisted_output_path.as_deref())
        .and_then(|path| Path::new(path).file_name().map(std::ffi::OsStr::to_owned))
        .map(|name| session_dir.join("tool-results").join(name));

    match spilled {
        Some(path) => match std::fs::read_to_string(&path) {
            Ok(whole) => {
                let total = whole.len() as u64;
                TurnItem::ToolResult {
                    tool,
                    head: whole,
                    total_bytes: total,
                    truncated: false,
                }
            }
            // A spill we cannot read is a smaller result, not a failed ingest:
            // keep the preview and say plainly that it is short.
            Err(_) => {
                let total = spill
                    .and_then(|result| result.persisted_output_size)
                    .unwrap_or(inline.len() as u64);
                let head = spill
                    .and_then(|result| result.stdout.clone())
                    .unwrap_or(inline);
                let truncated = (head.len() as u64) < total;
                TurnItem::ToolResult {
                    tool,
                    head,
                    total_bytes: total,
                    truncated,
                }
            }
        },
        None => {
            let total = inline.len() as u64;
            TurnItem::ToolResult {
                tool,
                head: inline,
                total_bytes: total,
                truncated: false,
            }
        }
    }
}

/// A tool result's own content, which is a string or a block array.
fn flatten(content: &Content) -> String {
    match content {
        Content::Text(text) => text.clone(),
        Content::Blocks(blocks) => {
            let mut out = String::new();
            for block in blocks {
                match block {
                    Block::Text { text } => push_prose(&mut out, text),
                    Block::Thinking { thinking } => push_prose(&mut out, thinking),
                    _ => {}
                }
            }
            out
        }
    }
}

/// Append a block's prose, keeping blocks separated and skipping empty ones.
fn push_prose(body: &mut String, text: &str) {
    if text.is_empty() {
        return;
    }
    if !body.is_empty() {
        body.push('\n');
    }
    body.push_str(text);
}

/// Fold a later block of an already-open message into its turn.
fn extend(record: &mut RawRecord, body: String, items: Vec<TurnItem>) {
    if !body.is_empty() {
        if !record.body.is_empty() {
            record.body.push('\n');
        }
        record.body.push_str(&body);
    }
    record.items.extend(items);
}

fn record(
    line: &Line,
    role: TurnRole,
    source: TurnSource,
    body: String,
    items: Vec<TurnItem>,
) -> RawRecord {
    RawRecord {
        ordinal: line.uuid.clone().unwrap_or_default(),
        parent_ordinal: line.parent_uuid.clone(),
        at: line
            .timestamp
            .clone()
            .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_owned()),
        role,
        source,
        body,
        items,
        // Claude's records carry `gitBranch` but never a commit sha, and a
        // branch name is not the thing `head_sha` promises.
        head_sha: None,
    }
}
