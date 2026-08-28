//! Turning what a native store wrote into the turns the intake already accepts.
//!
//! Two pure jobs, no IO, no clock, no store — plan 005's u2 unit. Given the
//! records a reader produced and what the durable ledger has already seen,
//! this decides WHAT to append and under WHICH numbers, and it shapes each
//! turn's payload to the measured policy on the way through.
//!
//! # The rescan is the reason this exists
//!
//! A reader that finds its file rotated or truncated cannot resume: it starts
//! from zero and reports [`ReadBatch::rescanned`], and what it returns is the
//! WHOLE conversation rather than a delta. Appended blindly that duplicates
//! every turn the conversation ever had — and it looks exactly like a busy
//! session, so nothing downstream would flag it. [`prepare_batch`] dedupes on
//! [`RawRecord::ordinal`], the store's own natural id, which is the only thing
//! that still distinguishes a stored record from a new one after the byte
//! offsets have been thrown away.
//!
//! # Why the payload policy lives HERE and not in the daemon
//!
//! [`shape_turn`] and [`OUTPUT_HEAD_BYTES`] were private to
//! `fs3_daemon::conversations`, where intake enforces them. The importer must
//! apply the same rules — it is cheaper about it, having the raw transcript in
//! hand — and a second implementation of a truncation rule is a rule that
//! drifts, which is plan 005's own risk r3. Prime ruled on 2026-08-28 that the
//! policy MOVES here as public functions and intake DELEGATES, so there is one
//! implementation and the daemon keeps its backstop. Behaviour is unchanged:
//! the daemon's own intake tests are the regression oracle and pass unmodified.
//!
//! # What is deliberately dropped
//!
//! [`RawRecord::parent_ordinal`] has nowhere to go: [`Turn`] carries no parent,
//! because v1 stores sequence and not the reply chain (req-0026 — `turn_no` IS
//! the navigation axis). Dropping it is a decision, not an oversight, and
//! [`normalize_record`] says so where it happens.

use std::collections::BTreeSet;

use crate::conversation::{ToolInput, Turn, TurnItem};
use crate::conversation_source::RawRecord;

/// How much of a tool result is kept (workshop 005, C2).
///
/// A constant rather than a config knob, per the workshop's own sketch:
/// measured, this keeps 62.7% of results whole and the opening lines of every
/// error — errors front-load — at 35.6% of the output bytes. It becomes a knob
/// the day someone has a number that beats it, and not before.
///
/// SINGLE-SOURCED HERE (prime ruling, 2026-08-28). `fs3_daemon::conversations`
/// re-exports this rather than defining its own; two definitions of a cut are
/// two cuts that can disagree.
pub const OUTPUT_HEAD_BYTES: usize = 512;

/// Tools whose input is the file they are about to write.
///
/// Measured at 1.2MB of a 2.85MB input side: the body is the very next commit,
/// so storing it here doubles the input bill for zero search value (C3). The
/// PATH and the size are kept, because "which file, how big" is the part a
/// search is ever going to ask about.
const WRITE_FAMILY: [&str; 6] = [
    "write",
    "edit",
    "create",
    "str_replace",
    "apply_patch",
    "multiedit",
];

/// One poll's worth of work, decided.
///
/// Borrows its ordinals from the batch it was prepared from: the caller still
/// holds those records while it writes, and a whole-conversation rescan is
/// thousands of them — cloning every id to hand it straight back to Postgres
/// is a per-turn allocation for nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedBatch<'a> {
    /// The turns to append, numbered densely from the ledger's high-water
    /// mark, payload policy already applied.
    pub turns: Vec<Turn>,
    /// `(ordinal, turn_no)` for each turn above — the ledger rows that make
    /// the next rescan a no-op. Written in the same transaction as the cursor.
    pub ledger: Vec<(&'a str, u32)>,
    /// How many records the ledger had already seen. On a `rescanned` batch of
    /// an unchanged conversation this is every record, and `turns` is empty.
    pub deduped: usize,
}

impl PreparedBatch<'_> {
    /// Whether this poll found anything worth writing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.turns.is_empty()
    }
}

/// Decide what to append from one batch, and under which numbers.
///
/// `seen` is the ledger's answer for exactly these records' ordinals, and
/// `next_turn_no` its high-water mark plus one — both from one consistent
/// snapshot, so the numbering cannot interleave with another poll.
///
/// Dedupes WITHIN the batch as well as against the ledger: an ordinal is
/// unique per session by construction, so a repeat inside one batch would
/// otherwise be stored twice under two different `turn_no`s — and the
/// `(conversation_id, turn_no)` primary key cannot catch that, because the two
/// keys genuinely differ.
#[must_use]
pub fn prepare_batch<'a>(
    records: &'a [RawRecord],
    seen: &BTreeSet<String>,
    next_turn_no: u32,
) -> PreparedBatch<'a> {
    let mut turns = Vec::with_capacity(records.len());
    let mut ledger = Vec::with_capacity(records.len());
    let mut minted: BTreeSet<&str> = BTreeSet::new();
    let mut deduped = 0;
    let mut turn_no = next_turn_no;

    for record in records {
        let ordinal = record.ordinal.as_str();
        if seen.contains(ordinal) || !minted.insert(ordinal) {
            deduped += 1;
            continue;
        }
        turns.push(normalize_record(record, turn_no));
        ledger.push((ordinal, turn_no));
        turn_no += 1;
    }

    PreparedBatch {
        turns,
        ledger,
        deduped,
    }
}

/// One record as the turn the store accepts, shaped.
///
/// Genuinely arithmetic: the reader already translated its store's dialect into
/// [`crate::TurnRole`], [`crate::TurnSource`] and [`TurnItem`], so nothing here
/// interprets a format. It assigns the number, applies the payload policy, and
/// drops what v1 does not store.
///
/// `parent_ordinal` is DROPPED deliberately: [`Turn`] has no parent field
/// because v1 stores sequence rather than the reply chain (req-0026). If the
/// chain is ever wanted it is a schema change and a stop-and-ask, not a field
/// somebody quietly adds here.
#[must_use]
pub fn normalize_record(record: &RawRecord, turn_no: u32) -> Turn {
    shape_turn(Turn {
        turn_no,
        role: record.role,
        source: record.source,
        head_sha: record.head_sha.clone(),
        at: record.at.clone(),
        body: record.body.clone(),
        items: record.items.clone(),
    })
}

/// Apply workshop 005's payload rulings to one turn.
///
/// Idempotent: shaping an already-shaped turn changes nothing, which is what
/// lets the importer shape cheaply and intake enforce without double-cutting.
#[must_use]
pub fn shape_turn(mut turn: Turn) -> Turn {
    for item in &mut turn.items {
        match item {
            TurnItem::ToolCall { tool, input } => {
                if let ToolInput::Verbatim { text } = input
                    && is_write_family(tool)
                {
                    *input = ToolInput::Elided {
                        path: first_line(text).to_string(),
                        bytes: text.len() as u64,
                    };
                }
            }
            TurnItem::ToolResult {
                head,
                total_bytes,
                truncated,
                ..
            } => {
                // `total_bytes` describes the WHOLE result, so it is only ours
                // to set when this is the first cut: a client that already
                // truncated knows a number we cannot recover.
                if head.len() > OUTPUT_HEAD_BYTES {
                    if !*truncated {
                        *total_bytes = head.len() as u64;
                    }
                    head.truncate(floor_char_boundary(head, OUTPUT_HEAD_BYTES));
                    *truncated = true;
                }
            }
        }
    }
    turn
}

/// Whether this tool's input is a file body we are about to commit anyway.
///
/// Matched on the tool name's last segment and case-insensitively, because
/// harnesses spell the same tool `Write`, `write`, `str_replace_editor` and
/// `fs.write` — and a policy that only catches one spelling is a policy that
/// silently stores the bodies from the others.
fn is_write_family(tool: &str) -> bool {
    let name = tool.rsplit(['.', '/', ':']).next().unwrap_or(tool);
    WRITE_FAMILY
        .iter()
        .any(|family| name.eq_ignore_ascii_case(family) || starts_with_family(name, family))
}

fn starts_with_family(name: &str, family: &str) -> bool {
    name.len() > family.len()
        && name.is_char_boundary(family.len())
        && name[..family.len()].eq_ignore_ascii_case(family)
        && !name.as_bytes()[family.len()].is_ascii_alphanumeric()
}

/// The first line, which for a write-family call is where the path is.
fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or("").trim()
}

/// The largest cut at or below `limit` that does not split a character.
///
/// `String::truncate` panics on a byte index inside a multi-byte character, and
/// a transcript is exactly where one appears — the 512th byte of a tool result
/// lands mid-character sooner or later, and a panic in intake would lose the
/// whole batch.
fn floor_char_boundary(text: &str, limit: usize) -> usize {
    let mut cut = limit.min(text.len());
    while cut > 0 && !text.is_char_boundary(cut) {
        cut -= 1;
    }
    cut
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversation::{TurnRole, TurnSource};

    fn record(ordinal: &str, body: &str) -> RawRecord {
        RawRecord {
            ordinal: ordinal.to_string(),
            parent_ordinal: Some("parent".to_string()),
            at: "2026-08-28T09:00:00Z".to_string(),
            role: TurnRole::Agent,
            source: TurnSource::System,
            body: body.to_string(),
            items: Vec::new(),
            head_sha: Some("abc123".to_string()),
        }
    }

    fn seen(ordinals: &[&str]) -> BTreeSet<String> {
        ordinals.iter().map(|o| (*o).to_string()).collect()
    }

    fn call(tool: &str, text: &str) -> TurnItem {
        TurnItem::ToolCall {
            tool: tool.to_string(),
            input: ToolInput::Verbatim {
                text: text.to_string(),
            },
        }
    }

    fn turn_with(items: Vec<TurnItem>) -> Turn {
        Turn {
            turn_no: 1,
            role: TurnRole::Agent,
            source: TurnSource::System,
            head_sha: None,
            at: "2026-08-27T09:00:00Z".to_string(),
            body: String::new(),
            items,
        }
    }

    // ═══ THE LEDGER ═══════════════════════════════════════════════════════

    /// THE case this unit exists for. A reader that rotated restarts from zero
    /// and hands back the WHOLE conversation with `rescanned = true`. If the
    /// ledger stays silent that is indistinguishable from a burst of new turns,
    /// and the conversation is stored twice.
    #[test]
    fn a_rescan_of_an_unchanged_conversation_appends_nothing() {
        let whole = [record("a", "one"), record("b", "two"), record("c", "three")];
        let first = prepare_batch(&whole, &BTreeSet::new(), 1);
        assert_eq!(first.turns.len(), 3, "the first ingest stores everything");

        let ledger = seen(&["a", "b", "c"]);
        let rescan = prepare_batch(&whole, &ledger, 4);

        assert!(
            rescan.is_empty(),
            "a rescan of an unchanged conversation must append ZERO turns — \
             silence here duplicates an entire conversation"
        );
        assert_eq!(rescan.deduped, 3);
        assert!(rescan.ledger.is_empty(), "and adds no ledger rows");
    }

    /// The other half: a rescan that genuinely grew stores only the growth, and
    /// numbers it after what is already there.
    #[test]
    fn a_rescan_that_grew_appends_only_the_new_records() {
        let whole = [
            record("a", "one"),
            record("b", "two"),
            record("c", "three"),
            record("d", "four"),
        ];
        let prepared = prepare_batch(&whole, &seen(&["a", "b"]), 3);

        assert_eq!(prepared.deduped, 2);
        assert_eq!(prepared.turns.len(), 2);
        assert_eq!(prepared.turns[0].body, "three");
        assert_eq!(
            prepared.turns[0].turn_no, 3,
            "numbering continues from the ledger's high-water mark"
        );
        assert_eq!(prepared.turns[1].turn_no, 4);
        assert_eq!(prepared.ledger, vec![("c", 3), ("d", 4)]);
    }

    /// An ordinal is unique per session, so a repeat inside ONE batch is a
    /// reader bug — but storing it twice under two numbers is a duplicate the
    /// `(conversation_id, turn_no)` key cannot catch, because the keys differ.
    #[test]
    fn a_repeated_ordinal_within_one_batch_is_stored_once() {
        let batch = [record("a", "one"), record("a", "one again")];
        let prepared = prepare_batch(&batch, &BTreeSet::new(), 1);

        assert_eq!(prepared.turns.len(), 1);
        assert_eq!(prepared.deduped, 1);
        assert_eq!(prepared.ledger, vec![("a", 1)]);
    }

    #[test]
    fn a_delta_is_numbered_densely_from_the_high_water_mark() {
        let batch = [record("x", "new")];
        let prepared = prepare_batch(&batch, &BTreeSet::new(), 118);

        assert_eq!(prepared.turns[0].turn_no, 118);
        assert_eq!(prepared.ledger, vec![("x", 118)]);
    }

    // ═══ THE NORMALIZER ═══════════════════════════════════════════════════

    #[test]
    fn a_record_carries_its_metadata_across_unchanged() {
        let turn = normalize_record(&record("a", "hello"), 7);

        assert_eq!(turn.turn_no, 7);
        assert_eq!(turn.role, TurnRole::Agent);
        assert_eq!(turn.source, TurnSource::System);
        assert_eq!(turn.at, "2026-08-28T09:00:00Z");
        assert_eq!(turn.body, "hello");
        assert_eq!(turn.head_sha.as_deref(), Some("abc123"));
    }

    /// `parent_ordinal` is dropped on purpose — v1 stores sequence, not the
    /// reply chain. This test exists so the drop stays a decision.
    #[test]
    fn the_parent_chain_is_dropped_because_v1_stores_sequence() {
        let source = record("a", "hello");
        assert!(source.parent_ordinal.is_some(), "the record HAS a parent");

        let turn = normalize_record(&source, 1);
        // `Turn` has no parent field at all; this is the whole assertion, and
        // it becomes a compile error the day someone adds one without ruling it.
        assert_eq!(turn.body, "hello");
    }

    // ═══ THE PAYLOAD POLICY ═══════════════════════════════════════════════
    //
    // Moved from `fs3_daemon::conversations` under prime's 2026-08-28 ruling.
    // These mirror the daemon's own intake tests, which are the regression
    // oracle for the move and must stay green unmodified.

    #[test]
    fn a_write_family_body_is_reduced_to_its_path_and_length() {
        let body = "crates/store/src/lib.rs\n".to_string() + &"x".repeat(50_000);
        let shaped = shape_turn(turn_with(vec![call("write", &body)]));

        let TurnItem::ToolCall { input, .. } = &shaped.items[0] else {
            panic!("still a call");
        };
        assert_eq!(
            *input,
            ToolInput::Elided {
                path: "crates/store/src/lib.rs".to_string(),
                bytes: body.len() as u64,
            },
            "the body is the very next commit; storing it here doubles the bill"
        );
    }

    #[test]
    fn the_write_family_is_matched_however_a_harness_spells_it() {
        for tool in [
            "write",
            "Write",
            "fs.write",
            "str_replace",
            "str_replace_editor",
            "edit",
            "Edit",
            "apply_patch",
        ] {
            let shaped = shape_turn(turn_with(vec![call(tool, "a/path.rs\nbody")]));
            let TurnItem::ToolCall { input, .. } = &shaped.items[0] else {
                panic!("still a call");
            };
            assert!(
                matches!(input, ToolInput::Elided { .. }),
                "{tool} is write-family"
            );
        }
    }

    #[test]
    fn a_reading_tool_keeps_its_input_verbatim() {
        for tool in ["read", "grep", "bash", "rewrite_history", "editor_config"] {
            let shaped = shape_turn(turn_with(vec![call(tool, "AGENTS.md\nwhatever")]));
            let TurnItem::ToolCall { input, .. } = &shaped.items[0] else {
                panic!("still a call");
            };
            assert!(
                matches!(input, ToolInput::Verbatim { .. }),
                "{tool} is not write-family"
            );
        }
    }

    /// The mutation check: remove the cut in [`shape_turn`] and this fails on
    /// the length, the total AND the flag. A golden that would still pass
    /// without the truncation is not a test.
    #[test]
    fn an_oversized_tool_result_is_cut_to_its_head_and_says_so() {
        let whole = "e".repeat(5_000);
        let shaped = shape_turn(turn_with(vec![TurnItem::ToolResult {
            tool: "bash".to_string(),
            head: whole.clone(),
            total_bytes: 0,
            truncated: false,
        }]));

        let TurnItem::ToolResult {
            head,
            total_bytes,
            truncated,
            ..
        } = &shaped.items[0]
        else {
            panic!("still a result");
        };
        assert_eq!(head.len(), OUTPUT_HEAD_BYTES);
        assert_eq!(*total_bytes, whole.len() as u64, "the size is not lost");
        assert!(*truncated);
    }

    /// A result that already fits is left entirely alone — the policy is a cut,
    /// not a rewrite.
    #[test]
    fn a_result_within_the_head_is_untouched() {
        let shaped = shape_turn(turn_with(vec![TurnItem::ToolResult {
            tool: "bash".to_string(),
            head: "short".to_string(),
            total_bytes: 5,
            truncated: false,
        }]));

        assert_eq!(
            shaped.items[0],
            TurnItem::ToolResult {
                tool: "bash".to_string(),
                head: "short".to_string(),
                total_bytes: 5,
                truncated: false,
            }
        );
    }

    #[test]
    fn a_clients_own_total_survives_enforcement() {
        let shaped = shape_turn(turn_with(vec![TurnItem::ToolResult {
            tool: "bash".to_string(),
            head: "e".repeat(1_000),
            total_bytes: 9_000_000,
            truncated: true,
        }]));

        let TurnItem::ToolResult {
            total_bytes, head, ..
        } = &shaped.items[0]
        else {
            panic!("still a result");
        };
        assert_eq!(*total_bytes, 9_000_000);
        assert_eq!(head.len(), OUTPUT_HEAD_BYTES, "but the head is still cut");
    }

    #[test]
    fn shaping_an_already_shaped_turn_changes_nothing() {
        let once = shape_turn(turn_with(vec![
            call("write", "a.rs\nbody"),
            TurnItem::ToolResult {
                tool: "bash".to_string(),
                head: "e".repeat(2_000),
                total_bytes: 0,
                truncated: false,
            },
        ]));
        assert_eq!(shape_turn(once.clone()), once);
    }

    /// The 512th byte lands mid-character sooner or later, and `truncate`
    /// panics there — which would lose the whole batch, not one result.
    #[test]
    fn a_cut_never_splits_a_character() {
        let shaped = shape_turn(turn_with(vec![TurnItem::ToolResult {
            tool: "bash".to_string(),
            // 3 bytes each, so 512 is not a boundary.
            head: "☃".repeat(1_000),
            total_bytes: 0,
            truncated: false,
        }]));

        let TurnItem::ToolResult { head, .. } = &shaped.items[0] else {
            panic!("still a result");
        };
        assert!(head.len() <= OUTPUT_HEAD_BYTES);
        assert!(
            head.len() > OUTPUT_HEAD_BYTES - 4,
            "and cuts as late as it can"
        );
        assert!(head.chars().all(|c| c == '☃'));
    }

    /// Every width of multi-byte character, because 512 mod 2, 3 and 4 are all
    /// different and only one of them is a boundary by luck.
    #[test]
    fn a_cut_lands_on_a_boundary_for_every_character_width() {
        for (glyph, width) in [("é", 2), ("☃", 3), ("𝄞", 4)] {
            let shaped = shape_turn(turn_with(vec![TurnItem::ToolResult {
                tool: "bash".to_string(),
                head: glyph.repeat(1_000),
                total_bytes: 0,
                truncated: false,
            }]));

            let TurnItem::ToolResult { head, .. } = &shaped.items[0] else {
                panic!("still a result");
            };
            assert_eq!(
                head.len() % width,
                0,
                "{glyph} is {width} bytes wide, so the cut must be a multiple of it"
            );
            assert!(head.len() <= OUTPUT_HEAD_BYTES);
            assert!(head.len() > OUTPUT_HEAD_BYTES - width);
            assert!(head.chars().all(|c| glyph.starts_with(c)));
        }
    }

    /// The normalizer applies the policy, not just the intake backstop: a
    /// record arriving with a 5KB tool result must already be cut by the time
    /// it is a `Turn`.
    #[test]
    fn normalizing_applies_the_payload_policy() {
        let mut source = record("a", "ran a command");
        source.items = vec![TurnItem::ToolResult {
            tool: "bash".to_string(),
            head: "e".repeat(5_000),
            total_bytes: 0,
            truncated: false,
        }];

        let turn = normalize_record(&source, 1);

        let TurnItem::ToolResult {
            head, truncated, ..
        } = &turn.items[0]
        else {
            panic!("still a result");
        };
        assert_eq!(head.len(), OUTPUT_HEAD_BYTES);
        assert!(*truncated);
    }
}
