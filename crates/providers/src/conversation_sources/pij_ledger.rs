//! Reading the pij seat ledger, `~/.pij/<seat>/events.ndjson` (plan 005, u1b).
//!
//! The ledger is the odd store of the four: it is keyed by SEAT rather than by
//! session uuid, it is the only place delivery RECEIPTS exist, and it carries a
//! monotonic `seq` on every record.
//!
//! # THE ORDINAL DERIVATION IS FROZEN
//!
//! `RawRecord::ordinal` is **the `seq` field rendered as a decimal string** —
//! `118`, not `"seq-118"`, not the integer, not a hash of the line.
//!
//! Changing that is not a refactor and not a cleanup. The ordinal is the key
//! the durable cursor ledger deduplicates on, it is written to Postgres, and it
//! outlives every process here. Derive it differently and every already-stored
//! record looks brand new on the next poll, so **the conversation silently
//! doubles** — and there is no clean recovery, because forgetting the session
//! re-reads from zero and duplicates it again. If you have found a reason to
//! change it, that is a message to the plan's PM before you ship and a plan
//! after. (Fleet rule, 2026-08-28, from u2 which owns the durable side.)
//!
//! It is also pinned externally: the committed expectations are generated with
//! `jsonl_structural(..., id_key="seq")`, which stringifies, so any other
//! rendering fails `assert_ordinals_are_a_subsequence`.
//!
//! # Why the cursor is a sequence and not a byte offset
//!
//! [`SourceCursor::Seq`] survives the file being rewritten entirely, which a
//! byte offset does not. That is the whole reason the variant exists, and it is
//! why this reader reports `rescanned: false` unconditionally: there is no
//! rotation for a seq cursor to be confused by. The cost is a full-file re-read
//! per poll with an O(1)-per-line filter. Ruled acceptable by the plan's PM on
//! 2026-08-28: the ledger is small, `seq` is the store's only monotonic key,
//! and a second cursor mechanism inside one reader is complexity bought against
//! a number nobody has measured a need for.
//!
//! Framing still comes from [`super::tail::read_lines`] — a live ledger can be
//! mid-line at read time exactly like any other jsonl store, and a torn last
//! line must yield nothing rather than half a record.
//!
//! # Items come from the dedicated events, never from the message blocks
//!
//! The ledger records a tool twice: once as an assistant `message` whose
//! content carries `toolCall` blocks, and once as a first-class `tool_call`
//! event. Mapping both would double every tool in the index, exactly as omp's
//! `tool_execution_start` mirror would. The dedicated events win, because they
//! also carry the `toolCallId` that pairs a result to its call.

use std::path::{Path, PathBuf};

use fs3_core::{
    ConversationSource, Error, Harness, IngestInput, RawRecord, ReadBatch, Result, SessionFile,
    SessionKind, SourceCursor, ToolInput, TurnItem, TurnRole, TurnSource,
};

use super::tail;

/// The wire convention that marks a peer-injected user turn.
///
/// A HEURISTIC over a convention, not a store field. A user record that does
/// not match falls through to a plain human turn rather than erroring.
const PEER_PREFIX: &str = "[pij from";

/// How far into a user turn the peer marker is looked for, matching the oracle.
const PEER_PREFIX_WINDOW: usize = 200;

/// Reads conversations out of a pij data root.
///
/// The root is injected rather than discovered so a test can point at a scratch
/// directory; the default is the real `~/.pij`.
#[derive(Clone, Debug)]
pub struct PijLedgerSource {
    root: PathBuf,
}

impl PijLedgerSource {
    /// A reader over an explicit pij root.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// A reader over the conventional `~/.pij` beneath `home`.
    #[must_use]
    pub fn from_home(home: impl AsRef<Path>) -> Self {
        Self {
            root: home.as_ref().join(".pij"),
        }
    }

    /// The pij root this reader was built over.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Where a seat's ledger lives.
    #[must_use]
    pub fn ledger_path(&self, seat: &str) -> PathBuf {
        self.root.join(seat).join("events.ndjson")
    }

    /// The seat this input addresses.
    ///
    /// The seat IS the address for this store, so both input routes land the
    /// same place — there is no uuid to join through.
    fn seat<'a>(&self, input: &'a IngestInput) -> Result<&'a str> {
        match input {
            IngestInput::Pij { id, .. } => Ok(id),
            IngestInput::Native {
                session_id,
                harness: Harness::PijLedger,
                ..
            } => Ok(session_id),
            IngestInput::Native { harness, .. } => Err(Error::Provider(format!(
                "the pij ledger reader was asked for a {harness} session"
            ))),
        }
    }
}

impl ConversationSource for PijLedgerSource {
    fn harness(&self) -> Harness {
        Harness::PijLedger
    }

    fn resolve(&self, input: &IngestInput) -> Result<Vec<SessionFile>> {
        let seat = self.seat(input)?;
        let path = self.ledger_path(seat);
        if !path.is_file() {
            return Err(Error::Provider(format!(
                "{}: seat {seat} has no ledger",
                path.display()
            )));
        }
        Ok(vec![SessionFile {
            path,
            session_id: seat.to_owned(),
            // A seat ledger has no children: a spawned peer gets its own seat
            // and therefore its own conversation.
            parent_session_id: None,
            kind: SessionKind::Main,
            harness: Harness::PijLedger,
        }])
    }

    fn read_incremental(
        &self,
        file: &SessionFile,
        cursor: Option<&SourceCursor>,
    ) -> Result<ReadBatch> {
        let held = resume_seq(cursor)?;

        // `None`, deliberately: framing is byte-oriented but our cursor is not,
        // so we re-frame the whole file and select on `seq`. This is what makes
        // the cursor survive a rewrite.
        let read = tail::read_lines(&file.path, None)?;

        let mut records = Vec::new();
        let mut highest = None;
        for line in &read.lines {
            let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            let Some(seq) = value.get("seq").and_then(serde_json::Value::as_u64) else {
                continue;
            };
            if held.is_some_and(|held| seq <= held) {
                continue;
            }
            highest = Some(highest.map_or(seq, |current: u64| current.max(seq)));
            if let Some(record) = record(seq, &value) {
                records.push(record);
            }
        }

        Ok(ReadBatch {
            records,
            cursor: advance(held, highest),
            // A sequence cursor cannot be rotated away from.
            rescanned: false,
        })
    }
}

/// The sequence a cursor resumes from, refusing another store's variant.
///
/// A foreign cursor read as zero would silently re-ingest an entire
/// conversation, so it is an error rather than a default. This is the ledger
/// half of the contract suite's foreign-cursor case; the byte-offset half lives
/// in [`super::tail::read_lines`], which refuses `Seq` symmetrically.
///
/// # Errors
/// [`Error::Provider`] when handed a [`SourceCursor::ByteOffset`] or
/// [`SourceCursor::RowId`].
pub fn resume_seq(cursor: Option<&SourceCursor>) -> Result<Option<u64>> {
    match cursor {
        None => Ok(None),
        Some(SourceCursor::Seq { seq }) => Ok(Some(*seq)),
        Some(SourceCursor::ByteOffset { .. }) => Err(foreign("byte-offset")),
        Some(SourceCursor::RowId { .. }) => Err(foreign("rowid")),
    }
}

fn foreign(kind: &str) -> Error {
    Error::Provider(format!(
        "the pij ledger resumes on a sequence; it was handed a {kind} cursor, and reading \
         that as zero would re-ingest the whole conversation"
    ))
}

/// Where the next read resumes, given what was held and what was consumed.
///
/// `highest` is the largest `seq` in this batch, or `None` for an empty poll.
/// An empty poll must return the cursor UNCHANGED — the contract suite asserts
/// equality, and a reader that rebuilt the cursor from an empty batch would
/// rewind to zero and re-ingest everything.
#[must_use]
pub fn advance(held: Option<u64>, highest: Option<u64>) -> SourceCursor {
    let seq = match (held, highest) {
        (None, None) => 0,
        (Some(held), None) => held,
        (None, Some(highest)) => highest,
        // `max`, not `highest`: a cursor must never move backwards.
        (Some(held), Some(highest)) => held.max(highest),
    };
    SourceCursor::Seq { seq }
}

/// How a delivery receipt is rendered as prose.
///
/// PINNED. This is SYNTHESISED text, not the store's own words: the ledger
/// records a receipt as three fields and nothing human. It will be embedded and
/// searched like any other turn, so a rendering that drifts between versions
/// makes two identical receipts read as two different turns. The shape matches
/// the reference oracle's, so a human diffing the two sees one string.
#[must_use]
pub fn receipt_body(to: &str, state: &str, message_id: &str) -> String {
    format!("→ {to}: delivery {state} ({message_id})")
}

/// One ledger event as a [`RawRecord`], or `None` when it is not a turn.
fn record(seq: u64, value: &serde_json::Value) -> Option<RawRecord> {
    let at = string(value, "timestamp")?;
    let data = value.get("data").unwrap_or(&serde_json::Value::Null);

    let (role, source, body, items) = match text(value, "type")? {
        "message" => message(data)?,
        "tool_call" => {
            let tool = string(data, "toolName").unwrap_or_default();
            let input = data.get("input").unwrap_or(&serde_json::Value::Null);
            (
                TurnRole::Agent,
                TurnSource::System,
                String::new(),
                vec![TurnItem::ToolCall {
                    tool,
                    input: ToolInput::Verbatim {
                        text: input.to_string(),
                    },
                }],
            )
        }
        "tool_result" => {
            let tool = string(data, "toolName").unwrap_or_default();
            let head = blocks_text(data.get("content"));
            let total_bytes = head.len() as u64;
            (
                TurnRole::Agent,
                TurnSource::System,
                head.clone(),
                vec![TurnItem::ToolResult {
                    tool,
                    head,
                    total_bytes,
                    truncated: false,
                }],
            )
        }
        "receipt" => (
            TurnRole::Agent,
            TurnSource::System,
            receipt_body(
                text(data, "to").unwrap_or_default(),
                text(data, "state").unwrap_or_default(),
                text(data, "messageId").unwrap_or_default(),
            ),
            Vec::new(),
        ),
        // The ledger grows event types; an unrecognised one is dropped, never
        // fatal. Three readers, one rule.
        _ => return None,
    };

    Some(RawRecord {
        // FROZEN: the decimal string form of `seq`. See the module docs.
        ordinal: seq.to_string(),
        // The ledger keeps no parent chain, which is why the committed
        // expectations pass `parent_key=None` for this store.
        parent_ordinal: None,
        at,
        role,
        source,
        body,
        items,
        head_sha: None,
    })
}

/// A `message` event's role, source and prose.
fn message(data: &serde_json::Value) -> Option<(TurnRole, TurnSource, String, Vec<TurnItem>)> {
    let message = data.get("message")?;
    let body = blocks_text(message.get("content"));
    match text(message, "role")? {
        "user" => {
            let source = if is_peer_injected(&body) {
                TurnSource::Peer
            } else {
                TurnSource::Human
            };
            Some((TurnRole::Human, source, body, Vec::new()))
        }
        // Assistant `toolCall` blocks are deliberately NOT mapped: the
        // dedicated `tool_call` events carry the same tools with their ids.
        _ => Some((TurnRole::Agent, TurnSource::System, body, Vec::new())),
    }
}

/// Every non-empty text block, in order.
///
/// A FOLD, not a `first()` — the same rule the omp reader follows, and for the
/// same reason: one block is the common case, not the only case this can
/// express. Deliberately duplicated rather than shared with the omp module, so
/// that no reader depends on another and the four worktrees still converge on a
/// trivial merge.
fn blocks_text(content: Option<&serde_json::Value>) -> String {
    let Some(blocks) = content.and_then(serde_json::Value::as_array) else {
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

    #[test]
    fn a_byte_offset_cursor_is_refused() {
        // omp's cursor handed to the ledger. Read as zero it would re-ingest
        // every turn the seat has ever emitted.
        let error = resume_seq(Some(&SourceCursor::ByteOffset {
            device: 1,
            inode: 2,
            offset: 3,
        }))
        .expect_err("a foreign cursor must be refused, not defaulted");
        assert!(
            error.to_string().contains("byte-offset"),
            "the refusal must name the cursor it was handed: {error}"
        );
    }

    #[test]
    fn a_rowid_cursor_is_refused() {
        let error = resume_seq(Some(&SourceCursor::RowId { rowid: 7 }))
            .expect_err("a foreign cursor must be refused, not defaulted");
        assert!(error.to_string().contains("rowid"), "{error}");
    }

    #[test]
    fn a_native_cursor_resumes() {
        assert_eq!(
            resume_seq(Some(&SourceCursor::Seq { seq: 167 })).expect("native cursor"),
            Some(167)
        );
        assert_eq!(resume_seq(None).expect("no cursor"), None);
    }

    #[test]
    fn an_empty_poll_leaves_the_cursor_exactly_where_it_was() {
        assert_eq!(advance(Some(167), None), SourceCursor::Seq { seq: 167 });
    }

    #[test]
    fn consuming_records_advances_to_the_highest_seq() {
        assert_eq!(
            advance(Some(120), Some(167)),
            SourceCursor::Seq { seq: 167 }
        );
        assert_eq!(advance(None, Some(167)), SourceCursor::Seq { seq: 167 });
    }

    #[test]
    fn the_cursor_never_moves_backwards() {
        assert_eq!(
            advance(Some(167), Some(120)),
            SourceCursor::Seq { seq: 167 }
        );
    }

    #[test]
    fn an_empty_ledger_still_yields_a_resumable_cursor() {
        assert_eq!(advance(None, None), SourceCursor::Seq { seq: 0 });
    }

    #[test]
    fn the_ledger_path_is_seat_scoped() {
        let source = PijLedgerSource::new("/tmp/pij-root");
        assert_eq!(
            source.ledger_path("pij-linguistic-narwhal"),
            Path::new("/tmp/pij-root/pij-linguistic-narwhal/events.ndjson")
        );
    }

    #[test]
    fn the_ordinal_is_the_decimal_string_form_of_seq() {
        // FROZEN. A different rendering makes every stored record look new and
        // silently doubles the conversation; it is also what the committed
        // expectations pin, via `jsonl_structural(id_key="seq")`.
        let record = record(
            122,
            &json(
                r#"{"seq":122,"type":"receipt","timestamp":"2026-08-27T00:00:00Z",
                    "data":{"messageId":"1787876005266-000001-43068","state":"queued",
                            "to":"pij-instant-lynx"}}"#,
            ),
        )
        .expect("a receipt is a turn");
        assert_eq!(record.ordinal, "122");
        assert_eq!(record.parent_ordinal, None);
    }

    #[test]
    fn the_receipt_rendering_is_pinned() {
        // Synthesised text that gets embedded and searched. A drift here makes
        // two identical receipts read as two different turns.
        assert_eq!(
            receipt_body(
                "pij-instant-lynx",
                "delivered",
                "1787876005266-000001-43068"
            ),
            "→ pij-instant-lynx: delivery delivered (1787876005266-000001-43068)"
        );
        let record = record(
            127,
            &json(
                r#"{"seq":127,"type":"receipt","timestamp":"2026-08-27T00:00:00Z",
                    "data":{"messageId":"1787876005266-000001-43068","state":"delivered",
                            "to":"pij-instant-lynx"}}"#,
            ),
        )
        .expect("a receipt is a turn");
        assert_eq!(
            record.body,
            "→ pij-instant-lynx: delivery delivered (1787876005266-000001-43068)"
        );
    }

    #[test]
    fn a_receipt_for_a_non_delivered_message_is_still_emitted() {
        // seq 122 is `queued`. The ledger is the only store in the fleet that
        // records delivery state, so a non-delivery is a fact worth keeping.
        let record = record(
            122,
            &json(
                r#"{"seq":122,"type":"receipt","timestamp":"2026-08-27T00:00:00Z",
                    "data":{"messageId":"m","state":"queued","to":"pij-instant-lynx"}}"#,
            ),
        )
        .expect("a queued receipt is still a turn");
        assert!(record.body.contains("queued"));
    }

    #[test]
    fn an_unknown_event_type_is_dropped_not_fatal() {
        assert!(
            record(
                200,
                &json(r#"{"seq":200,"type":"a_type_from_the_future","timestamp":"2026-08-27T00:00:00Z"}"#)
            )
            .is_none()
        );
    }

    #[test]
    fn a_peer_injected_user_turn_is_sourced_peer() {
        let record = record(
            119,
            &json(
                r#"{"seq":119,"type":"message","timestamp":"2026-08-27T00:00:00Z",
                    "data":{"message":{"role":"user","content":[
                        {"type":"text","text":"[pij from pij-instant-lynx] go"}]}}}"#,
            ),
        )
        .expect("a user message is a turn");
        assert_eq!(record.role, TurnRole::Human);
        assert_eq!(record.source, TurnSource::Peer);
    }

    #[test]
    fn a_user_turn_without_the_marker_falls_through_to_human() {
        let record = record(
            119,
            &json(
                r#"{"seq":119,"type":"message","timestamp":"2026-08-27T00:00:00Z",
                    "data":{"message":{"role":"user","content":[
                        {"type":"text","text":"just a person typing"}]}}}"#,
            ),
        )
        .expect("a user message is a turn");
        assert_eq!(record.source, TurnSource::Human);
    }

    #[test]
    fn every_text_block_survives_not_just_the_first() {
        let record = record(
            133,
            &json(
                r#"{"seq":133,"type":"message","timestamp":"2026-08-27T00:00:00Z",
                    "data":{"message":{"role":"toolResult","content":[
                        {"type":"text","text":"first half"},
                        {"type":"text","text":"second half"}]}}}"#,
            ),
        )
        .expect("a toolResult message is a turn");
        assert!(record.body.contains("first half"));
        assert!(
            record.body.contains("second half"),
            "block two was dropped: {:?}",
            record.body
        );
    }

    #[test]
    fn an_assistant_message_contributes_no_items() {
        // The dedicated `tool_call` events carry the tools. Mapping the message
        // blocks too would double every tool in the index.
        let record = record(
            129,
            &json(
                r#"{"seq":129,"type":"message","timestamp":"2026-08-27T00:00:00Z",
                    "data":{"message":{"role":"assistant","content":[
                        {"type":"thinking","thinking":"…"},
                        {"type":"toolCall","name":"bash","arguments":{"command":"ls"}}]}}}"#,
            ),
        )
        .expect("an assistant message is a turn");
        assert!(record.items.is_empty());
    }

    #[test]
    fn a_tool_call_event_carries_the_tool() {
        let record = record(
            118,
            &json(
                r#"{"seq":118,"type":"tool_call","timestamp":"2026-08-27T00:00:00Z",
                    "data":{"type":"tool_call","toolName":"bash",
                            "toolCallId":"toolu_01","input":{"command":"ls"}}}"#,
            ),
        )
        .expect("a tool_call is a turn");
        let [TurnItem::ToolCall { tool, input }] = &record.items[..] else {
            panic!("expected exactly one tool call: {:?}", record.items);
        };
        assert_eq!(tool, "bash");
        let ToolInput::Verbatim { text } = input else {
            panic!("a reader emits verbatim input; intake applies the payload policy");
        };
        assert!(text.contains("\"command\":\"ls\""));
    }

    #[test]
    fn a_tool_result_event_carries_its_output() {
        let record = record(
            120,
            &json(
                r#"{"seq":120,"type":"tool_result","timestamp":"2026-08-27T00:00:00Z",
                    "data":{"toolName":"bash","isError":false,
                            "content":[{"type":"text","text":"flowspace3-db  Up"}]}}"#,
            ),
        )
        .expect("a tool_result is a turn");
        let [
            TurnItem::ToolResult {
                tool,
                head,
                total_bytes,
                ..
            },
        ] = &record.items[..]
        else {
            panic!("expected exactly one tool result: {:?}", record.items);
        };
        assert_eq!(tool, "bash");
        assert_eq!(head, "flowspace3-db  Up");
        assert_eq!(*total_bytes, "flowspace3-db  Up".len() as u64);
    }
}
