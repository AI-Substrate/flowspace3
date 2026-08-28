//! Reading omp / pi native session jsonl (plan 005, unit u1b).
//!
//! An omp session is ONE file: `<sessions-root>/<slug>/<ts>_<uuid>.jsonl`,
//! append-only after its first 256 bytes, read incrementally through
//! [`super::tail::read_lines`]. What is dialect — and therefore what lives here
//! — is the record allowlist, the `xd://` tool remap, the toolCall/toolResult
//! pairing, the spilled-output resolution and the first-class compaction
//! record.
//!
//! # The slug is not claude's slug
//!
//! Measured while harvesting the fixtures (2026-08-28), correcting recipe §0:
//! omp's session directory STRIPS the home prefix. A workspace at
//! `/Users/agent/substrate/flowspace/flowspace3` is stored under
//! `-substrate-flowspace-flowspace3`, not the `-Users-agent-substrate-...` form
//! claude uses. A resolver built from the claude rule finds no directory at all,
//! which is a silent empty ingest rather than an error.
//!
//! # Unknown record types are dropped, never fatal
//!
//! omp emits types this allowlist has never heard of — `ttsr_injection`,
//! `branch_summary`, `service_tier_change` are all real and none appear in the
//! committed window. A store is free to grow a record type; a reader that
//! errors on one turns a routine harness upgrade into a dead ingest. Unknown
//! types are skipped and the surrounding records still parse.

use std::path::{Path, PathBuf};

use fs3_core::{
    ConversationSource, Error, Harness, IngestInput, RawRecord, ReadBatch, Result, SessionFile,
    SessionKind, SourceCursor, ToolInput, TurnItem, TurnRole, TurnSource,
};

use super::tail;

/// The marker that makes a toolCall an in-process tool rather than a file
/// operation.
const XD_SCHEME: &str = "xd://";

/// The wire convention that marks a peer-injected user turn.
///
/// A HEURISTIC over a convention, not a store field: omp records no "who put
/// this here" axis, and this prefix is what the fleet's own tooling writes. A
/// user record that does not match falls through to a plain human turn rather
/// than erroring — when the convention eventually does not hold, the reader
/// should degrade to a slightly less precise turn, not refuse the conversation.
const PEER_PREFIX: &str = "[pij from";

/// How far into a user turn the peer marker is looked for.
///
/// The oracle scans the first 200 characters; matching that keeps the two in
/// agreement about which turns are peer-injected.
const PEER_PREFIX_WINDOW: usize = 200;

/// Reads conversations out of an omp sessions root.
///
/// Both the sessions root and the home directory are injected rather than
/// discovered: the slug is derived by stripping home, and a slug that depends
/// on who is running the test is a slug that passes on exactly one machine.
#[derive(Clone, Debug)]
pub struct OmpSource {
    sessions_root: PathBuf,
    home: PathBuf,
}

impl OmpSource {
    /// A reader over an explicit sessions root and home directory.
    #[must_use]
    pub fn new(sessions_root: impl Into<PathBuf>, home: impl Into<PathBuf>) -> Self {
        Self {
            sessions_root: sessions_root.into(),
            home: home.into(),
        }
    }

    /// A reader over the conventional layout beneath `home`.
    #[must_use]
    pub fn from_home(home: impl Into<PathBuf>) -> Self {
        let home = home.into();
        Self {
            sessions_root: home.join(".omp/agent/sessions"),
            home,
        }
    }

    /// The sessions root this reader was built over.
    #[must_use]
    pub fn sessions_root(&self) -> &Path {
        &self.sessions_root
    }

    /// The native session id this input addresses.
    fn session_id<'a>(&self, input: &'a IngestInput) -> Result<&'a str> {
        match input {
            IngestInput::Native {
                session_id,
                harness: Harness::Omp,
                ..
            } => Ok(session_id),
            IngestInput::Native { harness, .. } => Err(Error::Provider(format!(
                "the omp reader was asked for a {harness} session"
            ))),
            IngestInput::Pij { id, .. } => Err(Error::Provider(format!(
                "seat {id} addresses the omp store only through the `pij sessions` join; \
                 resolve the native session id first"
            ))),
        }
    }
}

impl ConversationSource for OmpSource {
    fn harness(&self) -> Harness {
        Harness::Omp
    }

    fn resolve(&self, input: &IngestInput) -> Result<Vec<SessionFile>> {
        let session_id = self.session_id(input)?;
        let directory = self
            .sessions_root
            .join(session_slug(input.folder(), &self.home));

        // Re-read on EVERY call, per the trait: a session directory is live.
        let entries = std::fs::read_dir(&directory).map_err(|error| {
            Error::Provider(format!(
                "{}: cannot read the omp session directory: {error} — note that omp strips \
                 the home prefix from its slug, so claude's `-Users-...` form finds nothing",
                directory.display()
            ))
        })?;

        let suffix = format!("_{session_id}.jsonl");
        for entry in entries {
            let entry = entry.map_err(|error| {
                Error::Provider(format!(
                    "{}: cannot read a session directory entry: {error}",
                    directory.display()
                ))
            })?;
            if entry.file_name().to_string_lossy().ends_with(&suffix) {
                return Ok(vec![SessionFile {
                    path: entry.path(),
                    session_id: session_id.to_owned(),
                    // omp has no subagent conversations. The `<session>/`
                    // directory beside the file holds spilled tool OUTPUT,
                    // which is a payload, not a conversation with roles and a
                    // sequence.
                    parent_session_id: None,
                    kind: SessionKind::Main,
                    harness: Harness::Omp,
                }]);
            }
        }

        Err(Error::Provider(format!(
            "{}: holds no session ending {suffix}",
            directory.display()
        )))
    }

    fn read_incremental(
        &self,
        file: &SessionFile,
        cursor: Option<&SourceCursor>,
    ) -> Result<ReadBatch> {
        let read = tail::read_lines(&file.path, cursor)?;
        let mut records = Vec::with_capacity(read.lines.len());
        for line in &read.lines {
            // A line that is not a record at all is store corruption, not a
            // reason to fail the conversation: drop it, like an unknown type.
            let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            if let Some(record) = self.record(&file.path, &value) {
                records.push(record);
            }
        }
        Ok(ReadBatch {
            records,
            cursor: read.cursor,
            rescanned: read.rescanned,
        })
    }
}

impl OmpSource {
    /// One store record as a [`RawRecord`], or `None` when it is not a turn.
    fn record(&self, session_file: &Path, value: &serde_json::Value) -> Option<RawRecord> {
        // `title` is the 256-byte header slot and carries no `id` at all, so no
        // ordinal is even expressible for it. Every other drop is a judgement;
        // this one is arithmetic.
        let ordinal = string(value, "id")?;
        let at = string(value, "timestamp")?;
        let parent_ordinal = string(value, "parentId");

        let (role, source, body, items) = match text(value, "type")? {
            "message" => self.message(session_file, value)?,
            "compaction" => (
                TurnRole::Agent,
                // The harness rebuilt its own context; nobody spoke.
                TurnSource::System,
                string(value, "summary").unwrap_or_default(),
                Vec::new(),
            ),
            "custom_message" => (
                TurnRole::Agent,
                TurnSource::System,
                string(value, "content").unwrap_or_default(),
                Vec::new(),
            ),
            // `session`, `model_change`, `thinking_level_change` are not turns;
            // `custom`/`tool_execution_start` is the MIRROR of a call that is
            // already carried by the assistant record, and emitting both is
            // what makes a naive tool count double. Anything else is a type
            // this reader has not been taught — dropped, never fatal.
            _ => return None,
        };

        Some(RawRecord {
            ordinal,
            parent_ordinal,
            at,
            role,
            source,
            body,
            items,
            // omp records no repo HEAD.
            head_sha: None,
        })
    }

    /// A `message` record's role, source, prose and items.
    fn message(
        &self,
        session_file: &Path,
        value: &serde_json::Value,
    ) -> Option<(TurnRole, TurnSource, String, Vec<TurnItem>)> {
        let message = value.get("message")?;
        let body = prose(message);

        match text(message, "role")? {
            "user" => {
                let source = if is_peer_injected(&body) {
                    TurnSource::Peer
                } else {
                    TurnSource::Human
                };
                Some((TurnRole::Human, source, body, Vec::new()))
            }
            "assistant" => {
                let items = message
                    .get("content")
                    .and_then(serde_json::Value::as_array)
                    .map(|blocks| {
                        blocks
                            .iter()
                            .filter(|block| text(block, "type") == Some("toolCall"))
                            .map(|block| {
                                tool_call_item(
                                    text(block, "name").unwrap_or_default(),
                                    block.get("arguments").unwrap_or(&serde_json::Value::Null),
                                )
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                Some((TurnRole::Agent, TurnSource::System, body, items))
            }
            "toolResult" => {
                let tool = string(message, "toolName").unwrap_or_default();
                let item = self.tool_result_item(session_file, tool, &body);
                Some((TurnRole::Agent, TurnSource::System, body, vec![item]))
            }
            // omp writes `custom` role messages too; they are harness prose.
            _ => Some((TurnRole::Agent, TurnSource::System, body, Vec::new())),
        }
    }

    /// A tool result, resolved from its spill file when it has one.
    ///
    /// omp truncates oversized output ITSELF and spills the raw bytes beside
    /// the session, leaving an `artifact://<n>` marker inline. Unlike claude's
    /// persisted output, the inline text is NOT a prefix of the spilled file —
    /// measured 2026-08-28: the inline body abbreviates a git sha to seven
    /// characters where the file has forty and omits the `Author:` line
    /// entirely, and it carries TWO elisions in the middle rather than one cut
    /// tail. So a 512-byte head of each is different text, and storing the
    /// inline body loses content that nothing downstream can recover.
    ///
    /// When the spill file has been garbage-collected, fall back to the inline
    /// body and mark it `truncated` — a degraded turn, visibly degraded, beats
    /// failing an entire conversation because one tool result aged out.
    fn tool_result_item(&self, session_file: &Path, tool: String, body: &str) -> TurnItem {
        if let Some(artifact) = artifact_reference(body)
            && let Ok(Some(path)) = spill_path(session_file, artifact)
            && let Ok(raw) = std::fs::read_to_string(&path)
        {
            let total_bytes = raw.len() as u64;
            return TurnItem::ToolResult {
                tool,
                head: raw,
                total_bytes,
                truncated: false,
            };
        }

        let total_bytes = body.len() as u64;
        TurnItem::ToolResult {
            tool,
            head: body.to_owned(),
            // `artifact_reference` matching at all means the store already cut
            // this, so the head is short of the real output whatever we do.
            truncated: artifact_reference(body).is_some(),
            total_bytes,
        }
    }
}

/// omp's directory name for a workspace.
///
/// The absolute path with the home prefix removed and each component prefixed
/// by `-`, so `/Users/agent/substrate/flowspace/flowspace3` under home
/// `/Users/agent` becomes `-substrate-flowspace-flowspace3`. A folder outside
/// `home` keeps its whole path, which is the same rule with nothing to strip.
#[must_use]
pub fn session_slug(folder: &Path, home: &Path) -> String {
    let relative = folder.strip_prefix(home).unwrap_or(folder);
    let mut slug = String::with_capacity(1 + relative.as_os_str().len());
    for component in relative.components() {
        if let std::path::Component::Normal(part) = component {
            slug.push('-');
            slug.push_str(&part.to_string_lossy());
        }
    }
    slug
}

/// Every non-empty text block of a message, in order.
///
/// A FOLD, not a `first()`. No record in the committed fixture carries more
/// than one text block, but that is a fact about a sample, not an invariant of
/// the store — and a reader that took the first block would silently discard
/// the second the day omp emits one, with no error and no failing test. One
/// block is the common case, not the only case this can express.
///
/// `thinking` blocks are deliberately excluded: they are the model's scratch
/// space, not prose the store attributes to the turn.
#[must_use]
pub fn prose(message: &serde_json::Value) -> String {
    let Some(blocks) = message.get("content").and_then(serde_json::Value::as_array) else {
        return String::new();
    };
    let mut body = String::new();
    for block in blocks {
        if text(block, "type") != Some("text") {
            continue;
        }
        let Some(chunk) = text(block, "text") else {
            continue;
        };
        if chunk.trim().is_empty() {
            continue;
        }
        if !body.is_empty() {
            body.push_str("\n\n");
        }
        body.push_str(chunk);
    }
    body
}

/// Whether a user turn was injected by a peer rather than typed by a person.
#[must_use]
pub fn is_peer_injected(body: &str) -> bool {
    let window = &body[..body
        .char_indices()
        .nth(PEER_PREFIX_WINDOW)
        .map_or(body.len(), |(index, _)| index)];
    window.contains(PEER_PREFIX)
}

/// Turn one assistant `toolCall` content block into a [`TurnItem`].
///
/// # The `xd://` remap is keyed on the PATH, never on the tool's name
///
/// omp encodes its in-process tools as ordinary tool calls whose
/// `arguments.path` carries an `xd://<tool>` URL. Recipe gotcha 2 describes
/// these as virtual `write` calls, and the fixture shows why that spelling is
/// too narrow: of the five `xd://` calls in the committed window, four are
/// `write` and ONE IS A `read` (line 93). A rule keyed on `name == "write"`
/// misses it and reports a file read that never happened.
///
/// So the rule is a property of the arguments, not an accident of the sample:
/// any call whose `arguments.path` starts with `xd://` is an invocation of the
/// tool that path names.
///
/// # Why this is not cosmetic
///
/// The intake endpoint elides the WRITE FAMILY by tool name — a `ToolCall`
/// named `write` has its input replaced by `ToolInput::Elided { path, bytes }`
/// (`fs3_daemon::conversations::shape`). Leave an `xd://pij_send` call named
/// `write` and the index gains a fictional file edit whose "path" is the first
/// line of a pij message. Remapping the name to `pij_send` is what keeps that
/// policy pointed at real writes.
#[must_use]
pub fn tool_call_item(name: &str, arguments: &serde_json::Value) -> TurnItem {
    let path = arguments.get("path").and_then(serde_json::Value::as_str);
    let tool = match path.and_then(|path| path.strip_prefix(XD_SCHEME)) {
        Some(target) => target.to_owned(),
        None => name.to_owned(),
    };
    TurnItem::ToolCall {
        tool,
        input: ToolInput::Verbatim {
            text: verbatim_arguments(arguments),
        },
    }
}

/// A tool call's arguments as text, path first.
///
/// The path leads because the intake endpoint's write-family elision keeps the
/// FIRST LINE as the path it stores (`shape` calls `first_line`). Ordering it
/// anywhere else would hand that policy a fragment of JSON and call it a
/// filename.
fn verbatim_arguments(arguments: &serde_json::Value) -> String {
    match arguments.get("path").and_then(serde_json::Value::as_str) {
        Some(path) => format!("{path}\n{arguments}"),
        None => arguments.to_string(),
    }
}

/// The artifact id in a `[raw output: artifact://<n>]` marker, if there is one.
#[must_use]
pub fn artifact_reference(body: &str) -> Option<&str> {
    let rest = body.rsplit_once("artifact://")?.1;
    let end = rest
        .find(|character: char| !character.is_ascii_digit())
        .unwrap_or(rest.len());
    (end > 0).then(|| &rest[..end])
}

/// The sidecar directory omp spills oversized tool output into.
///
/// It is the session file's path with the `.jsonl` extension removed.
#[must_use]
pub fn spill_dir(session_file: &Path) -> PathBuf {
    session_file.with_extension("")
}

/// Resolve an `artifact://<n>` reference to the spilled file that holds it.
///
/// The extension varies in the real store — `9`, `10`, `11` and `85` are
/// `.bash.log` while `30`, `37`, `41` and `65` are `.bash-original.log` — so
/// the artifact id is the NUMERIC PREFIX and the lookup globs on it rather
/// than guessing a name.
///
/// Returns `None` when the spill file has aged out: a garbage-collected
/// artifact must degrade to the inline preview, never fail the conversation.
///
/// # Errors
/// [`Error::Provider`] when the sidecar directory exists but cannot be read.
pub fn spill_path(session_file: &Path, artifact_id: &str) -> Result<Option<PathBuf>> {
    let dir = spill_dir(session_file);
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(Error::Provider(format!(
                "{}: cannot read the spill directory: {error}",
                dir.display()
            )));
        }
    };

    for entry in entries {
        let entry = entry.map_err(|error| {
            Error::Provider(format!(
                "{}: cannot read a spill directory entry: {error}",
                dir.display()
            ))
        })?;
        let name = entry.file_name();
        if name
            .to_string_lossy()
            .split_once('.')
            .is_some_and(|(prefix, _)| prefix == artifact_id)
        {
            return Ok(Some(entry.path()));
        }
    }
    Ok(None)
}

fn text<'a>(value: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(serde_json::Value::as_str)
}

fn string(value: &serde_json::Value, key: &str) -> Option<String> {
    text(value, key).map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json(text: &str) -> serde_json::Value {
        serde_json::from_str(text).expect("test json must parse")
    }

    fn tool_of(item: &TurnItem) -> &str {
        match item {
            TurnItem::ToolCall { tool, .. } | TurnItem::ToolResult { tool, .. } => tool,
        }
    }

    fn source() -> OmpSource {
        OmpSource::new("/tmp/sessions", "/Users/agent")
    }

    #[test]
    fn the_xd_remap_is_keyed_on_the_path_not_the_tool_name() {
        // The whole point of the ruling: the fixture's five `xd://` calls are
        // four `write`s AND ONE `read`. Keying on the name catches four of
        // five, and the miss is silent — it becomes a file read in the index.
        for name in ["write", "read", "glob", "anything-at-all"] {
            let item = tool_call_item(
                name,
                &json(r#"{"path":"xd://pij_send","content":"hi","i":"Sending"}"#),
            );
            assert_eq!(
                tool_of(&item),
                "pij_send",
                "a toolCall with an xd:// path is an in-process tool invocation \
                 whatever the store named it; `{name}` was reported as a file operation"
            );
        }
    }

    #[test]
    fn an_ordinary_file_tool_keeps_its_own_name() {
        let item = tool_call_item(
            "read",
            &json(r#"{"path":"crates/core/src/lib.rs","i":"Reading"}"#),
        );
        assert_eq!(tool_of(&item), "read");
    }

    #[test]
    fn a_remapped_call_no_longer_matches_the_write_family() {
        // Downstream, `fs3_daemon::conversations::shape` elides any ToolCall
        // whose tool name is write-family. If the remap left this named
        // `write`, the index would gain a file edit whose path is the first
        // line of a pij message.
        let item = tool_call_item(
            "write",
            &json(r#"{"path":"xd://pij_send","content":"hi","i":"Sending"}"#),
        );
        assert_ne!(tool_of(&item), "write");
        assert_eq!(tool_of(&item), "pij_send");
    }

    #[test]
    fn a_write_family_call_leads_with_its_path() {
        // `shape` stores `first_line(text)` as the elided path, so a genuine
        // write must present its path first or the index records a fragment of
        // JSON as a filename.
        let item = tool_call_item(
            "write",
            &json(r#"{"path":"docs/notes.md","content":"body","i":"Writing"}"#),
        );
        let TurnItem::ToolCall {
            input: ToolInput::Verbatim { text },
            ..
        } = &item
        else {
            panic!("a toolCall must carry verbatim input before intake shapes it");
        };
        assert_eq!(text.lines().next(), Some("docs/notes.md"));
    }

    #[test]
    fn every_text_block_survives_not_just_the_first() {
        // PM3 ruling, 2026-08-28: taking `first()` would promote a fixture fact
        // to a store invariant and silently drop the second block forever.
        let body = prose(&json(
            r#"{"content":[
                {"type":"text","text":"first half"},
                {"type":"thinking","thinking":"ignored"},
                {"type":"text","text":"second half"}
            ]}"#,
        ));
        assert!(
            body.contains("first half"),
            "block one was dropped: {body:?}"
        );
        assert!(
            body.contains("second half"),
            "block two was dropped — this is the silent dropper the fold exists to prevent: \
             {body:?}"
        );
    }

    #[test]
    fn a_single_block_is_reproduced_exactly() {
        // The oracle hashes the store's verbatim text, so the common case must
        // gain no separator, no padding and no reordering.
        assert_eq!(
            prose(&json(r#"{"content":[{"type":"text","text":"just this"}]}"#)),
            "just this"
        );
    }

    #[test]
    fn thinking_blocks_are_not_prose() {
        assert_eq!(
            prose(&json(
                r#"{"content":[{"type":"thinking","thinking":"scratch"}]}"#
            )),
            ""
        );
    }

    #[test]
    fn a_peer_injected_user_turn_is_sourced_peer() {
        assert!(is_peer_injected("[pij from pij-pale-silkworm] go"));
    }

    #[test]
    fn a_user_turn_without_the_marker_falls_through_to_human() {
        // A HEURISTIC over a wire convention: when it does not hold, degrade to
        // a less precise turn rather than erroring.
        assert!(!is_peer_injected("just a person typing"));
    }

    #[test]
    fn the_peer_marker_is_only_honoured_near_the_start() {
        let late = format!("{}[pij from x]", "x".repeat(400));
        assert!(!is_peer_injected(&late));
    }

    #[test]
    fn an_unknown_record_type_is_dropped_not_fatal() {
        // omp really does emit `ttsr_injection`, `branch_summary` and
        // `service_tier_change`. A reader that errored on one would turn a
        // routine harness upgrade into a dead ingest.
        let dropped = source().record(
            Path::new("/tmp/s.jsonl"),
            &json(r#"{"type":"ttsr_injection","id":"aa","timestamp":"2026-08-26T07:46:01.430Z"}"#),
        );
        assert!(dropped.is_none());
    }

    #[test]
    fn a_record_without_an_id_is_dropped() {
        // This is the `title` slot: no id means no ordinal is expressible.
        let dropped = source().record(
            Path::new("/tmp/s.jsonl"),
            &json(r#"{"type":"title","title":"","updatedAt":"2026-08-26T07:46:01.430Z"}"#),
        );
        assert!(dropped.is_none());
    }

    #[test]
    fn a_tool_result_takes_the_record_level_iso_timestamp() {
        // Measured: the INNER `message.timestamp` is epoch-milliseconds while
        // the record-level one is ISO-8601. Keying on the inner field would
        // emit integers where the contract wants RFC 3339 — on 72 of 117
        // records, and it would still parse.
        let record = source()
            .record(
                Path::new("/tmp/s.jsonl"),
                &json(
                    r#"{"type":"message","id":"aa","parentId":"bb",
                        "timestamp":"2026-08-26T07:46:01.430Z",
                        "message":{"role":"toolResult","toolName":"bash",
                                   "timestamp":1787731213876,
                                   "content":[{"type":"text","text":"out"}]}}"#,
                ),
            )
            .expect("a toolResult is a turn");
        assert_eq!(record.at, "2026-08-26T07:46:01.430Z");
    }

    #[test]
    fn a_compaction_record_is_a_system_turn_carrying_its_summary() {
        // ac-0005. It also sits IN the parent chain, so dropping it breaks the
        // chain across the seam as well as losing the marker.
        let record = source()
            .record(
                Path::new("/tmp/s.jsonl"),
                &json(
                    // `r###`, not `r#` or `r##`: a real omp compaction summary
                    // opens with a markdown heading, so the bytes contain
                    // `"##` and would close either shorter literal.
                    r###"{"type":"compaction","id":"a932507b","parentId":"58a257ae",
                        "timestamp":"2026-08-26T08:14:24.462Z","summary":"## Goal",
                        "firstKeptEntryId":"068a550c","tokensBefore":117001}"###,
                ),
            )
            .expect("compaction is never dropped");
        assert_eq!(record.ordinal, "a932507b");
        assert_eq!(record.parent_ordinal.as_deref(), Some("58a257ae"));
        assert_eq!(record.source, TurnSource::System);
        assert_eq!(record.body, "## Goal");
    }

    #[test]
    fn the_slug_strips_the_home_prefix() {
        // Measured correction 1: this is NOT claude's `-Users-...` convention,
        // and building the omp path from the claude rule finds no directory.
        assert_eq!(
            session_slug(
                Path::new("/Users/agent/substrate/flowspace/flowspace3"),
                Path::new("/Users/agent"),
            ),
            "-substrate-flowspace-flowspace3"
        );
    }

    #[test]
    fn a_folder_outside_home_keeps_its_whole_path() {
        assert_eq!(
            session_slug(Path::new("/srv/checkouts/fs3"), Path::new("/Users/agent")),
            "-srv-checkouts-fs3"
        );
    }

    #[test]
    fn an_artifact_marker_is_recognised_and_bare_prose_is_not() {
        assert_eq!(
            artifact_reference("…[+338]\n[raw output: artifact://30]\n\nWall time: 0.05 seconds"),
            Some("30")
        );
        assert_eq!(artifact_reference("ordinary tool output"), None);
    }

    #[test]
    fn the_spill_directory_is_the_session_file_without_its_extension() {
        assert_eq!(
            spill_dir(Path::new("/s/2026-08-26T07-46-01-430Z_01a03d08.jsonl")),
            Path::new("/s/2026-08-26T07-46-01-430Z_01a03d08")
        );
    }

    #[test]
    fn a_missing_spill_file_degrades_to_the_inline_body() {
        // An artifact can be garbage-collected. Failing here would make a whole
        // conversation unreadable because one tool result aged out.
        let missing = std::env::temp_dir().join("fs3-omp-no-such-session.jsonl");
        assert_eq!(
            spill_path(&missing, "30").expect("absent is not a failure"),
            None
        );

        let item = source().tool_result_item(
            &missing,
            "bash".to_owned(),
            "preview…[raw output: artifact://30]",
        );
        let TurnItem::ToolResult {
            head, truncated, ..
        } = &item
        else {
            panic!("expected a tool result");
        };
        assert!(head.starts_with("preview"));
        assert!(
            *truncated,
            "a degraded result must SAY it is short of the real output"
        );
    }

    #[test]
    fn an_untruncated_result_is_not_marked_truncated() {
        let item = source().tool_result_item(
            Path::new("/tmp/fs3-omp-none.jsonl"),
            "bash".to_owned(),
            "complete output",
        );
        let TurnItem::ToolResult {
            truncated,
            total_bytes,
            ..
        } = &item
        else {
            panic!("expected a tool result");
        };
        assert!(!*truncated);
        assert_eq!(*total_bytes, "complete output".len() as u64);
    }
}
