//! The claude reader against its committed fixtures (plan 005, unit u1a).
//!
//! Everything here runs over a SCRATCH COPY in a temporary directory. The
//! committed fixtures are byte-pinned and two of the contract's cases write on
//! purpose — growing the session and tearing a record — so touching the real
//! ones would break every claim the expectations make about them.

use std::path::{Path, PathBuf};

use fs3_core::{
    ConversationSource, Harness, IngestInput, RawRecord, SessionKind, SourceCursor, TurnItem,
};
use fs3_providers::conversation_sources::claude::ClaudeSource;
use fs3_testkit::{
    Expectations, FixtureStore, SourceFixture, conversation_source_contract, fixtures_root,
};

/// The session with the subagent sidecar and the interrupted merge group.
const MAIN: &str = "a5a5588f-0979-439f-a1bf-ddf185a089c7";
/// The session that carries the spilled tool result.
const SPILLED: &str = "b1d6f4fb-bd8e-4a10-a018-4205f4058b8e";

/// A scratch directory that removes itself.
///
/// `tempfile` is not a dependency of this crate and would be one bought for a
/// handful of tests; `tail.rs` already set this precedent next door.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("the clock is after 1970")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("fs3-claude-{name}-{nanos}"));
        std::fs::create_dir_all(&path).expect("scratch dir");
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("scratch subdirectory");
    for entry in std::fs::read_dir(from).expect("readable fixture directory") {
        let entry = entry.expect("a fixture entry");
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).expect("copy a fixture file");
        }
    }
}

/// A writable copy of the committed claude fixtures.
fn scratch_store(name: &str) -> (Scratch, PathBuf) {
    let scratch = Scratch::new(name);
    let root = scratch.0.join("projects");
    copy_tree(&fixtures_root().join("claude"), &root);
    (scratch, root)
}

fn append(path: &Path, text: &str) {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .expect("open the scratch session for append");
    file.write_all(text.as_bytes()).expect("append");
}

/// Real `user` records the store actually wrote, taken from the OTHER committed
/// session so their uuids cannot collide with the one being grown.
///
/// Growing a fixture with invented records would prove the suite rather than
/// the reader, which the contract says in as many words.
fn donor_records() -> Vec<String> {
    let path = fixtures_root()
        .join("claude")
        .join(format!("{SPILLED}.jsonl"));
    let text = std::fs::read_to_string(path).expect("the donor session is committed");
    text.lines()
        .filter(|line| {
            let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
                return false;
            };
            value["type"] == "user"
                && value["message"]["content"]
                    .as_str()
                    .is_some_and(|content| !content.is_empty())
        })
        .map(str::to_owned)
        .collect()
}

fn read_all(source: &ClaudeSource, input: &IngestInput) -> Vec<RawRecord> {
    let files = source.resolve(input).expect("resolve");
    let main = files
        .iter()
        .find(|file| file.kind == SessionKind::Main)
        .expect("a main session file");
    source
        .read_incremental(main, None)
        .expect("a full read")
        .records
}

fn native(session_id: &str) -> IngestInput {
    IngestInput::Native {
        session_id: session_id.to_owned(),
        harness: Harness::Claude,
        folder: PathBuf::from("/workspace"),
    }
}

// ---------------------------------------------------------------- contract

struct ClaudeFixture {
    source: ClaudeSource,
    main: PathBuf,
    donors: Vec<String>,
    used: usize,
    pending: Option<String>,
    _scratch: Scratch,
}

impl ClaudeFixture {
    fn new() -> Self {
        let (scratch, root) = scratch_store("contract");
        let main = root.join(format!("{MAIN}.jsonl"));
        Self {
            source: ClaudeSource::new(&root),
            main,
            donors: donor_records(),
            used: 0,
            pending: None,
            _scratch: scratch,
        }
    }

    fn donor(&mut self) -> String {
        let record = self
            .donors
            .get(self.used)
            .expect("the donor session holds enough real user records")
            .clone();
        self.used += 1;
        record
    }
}

impl SourceFixture for ClaudeFixture {
    fn source(&self) -> &dyn ConversationSource {
        &self.source
    }

    fn input(&self) -> IngestInput {
        native(MAIN)
    }

    /// The main session plus its one subagent sidecar.
    fn expected_session_files(&self) -> usize {
        2
    }

    /// 13 merged assistant turns + 26 user turns. The 38 assistant RECORDS the
    /// store holds collapse to 13 turns, which is the whole point of the merge.
    fn expected_records(&self) -> usize {
        39
    }

    fn grow(&mut self) -> usize {
        let first = self.donor();
        let second = self.donor();
        append(&self.main, &format!("{first}\n{second}\n"));
        2
    }

    fn begin_partial_record(&mut self) -> bool {
        let record = self.donor();
        let split = record.len() / 2;
        append(&self.main, &record[..split]);
        self.pending = Some(record[split..].to_owned());
        true
    }

    fn finish_partial_record(&mut self) {
        let rest = self.pending.take().expect("a record was begun");
        append(&self.main, &format!("{rest}\n"));
    }
}

#[test]
fn the_claude_reader_satisfies_the_conversation_source_contract() {
    let mut fixture = ClaudeFixture::new();
    conversation_source_contract(&mut fixture);
}

// ------------------------------------------------------------ resolution

#[test]
fn resolve_finds_the_sidecar_and_names_its_parent() {
    let (_scratch, root) = scratch_store("resolve");
    let source = ClaudeSource::new(&root);

    let files = source.resolve(&native(MAIN)).expect("resolve");

    assert_eq!(files.len(), 2, "the main session and its one sidecar");
    let sidecar = files
        .iter()
        .find(|file| file.kind == SessionKind::Subagent)
        .expect("the committed sidecar");
    assert_eq!(sidecar.session_id, "agent-aa8ccc51dce0404a8");
    assert_eq!(
        sidecar.parent_session_id.as_deref(),
        Some(MAIN),
        "the sidecar's .meta.json does not carry a parent id — the directory is the only \
         place that link exists"
    );
}

#[test]
fn a_sidecar_that_appears_mid_session_is_found_by_the_next_resolve() {
    let (_scratch, root) = scratch_store("late-sidecar");
    let source = ClaudeSource::new(&root);
    assert_eq!(source.resolve(&native(SPILLED)).expect("resolve").len(), 1);

    // A subagent spawned after ingestion began.
    let subagents = root.join(SPILLED).join("subagents");
    std::fs::create_dir_all(&subagents).expect("subagent directory");
    std::fs::copy(
        fixtures_root()
            .join("claude")
            .join(MAIN)
            .join("subagents")
            .join("agent-aa8ccc51dce0404a8.jsonl"),
        subagents.join("agent-later.jsonl"),
    )
    .expect("seed a late sidecar");

    let files = source.resolve(&native(SPILLED)).expect("re-resolve");
    assert_eq!(
        files.len(),
        2,
        "resolve runs on EVERY poll; a reader that caches its first answer loses every \
         subagent spawned after ingestion began"
    );
}

#[test]
fn a_session_this_store_does_not_hold_is_refused() {
    let (_scratch, root) = scratch_store("missing");
    let source = ClaudeSource::new(&root);
    assert!(source.resolve(&native("not-a-session")).is_err());
}

#[test]
fn a_pij_seat_is_refused_rather_than_joined_here() {
    let (_scratch, root) = scratch_store("seat");
    let source = ClaudeSource::new(&root);
    let outcome = source.resolve(&IngestInput::Pij {
        id: "pij-frightened-mastodon".to_owned(),
        folder: PathBuf::from("/workspace"),
    });
    assert!(
        outcome.is_err(),
        "the seat-to-session join belongs to the orchestrator, not inside a claude dialect"
    );
}

// ----------------------------------------------------------------- merge

#[test]
fn assistant_blocks_merge_by_message_id_to_the_count_the_fixture_pins() {
    let (_scratch, root) = scratch_store("merge");
    let records = read_all(&ClaudeSource::new(&root), &native(MAIN));

    let expectations = Expectations::load(FixtureStore::Claude);
    let extras = &expectations.session(MAIN).extras;
    let assistant_records = extras["assistant_records"].as_u64().expect("pinned");
    let distinct = extras["distinct_assistant_message_ids"]
        .as_u64()
        .expect("pinned");

    let merged = records
        .iter()
        .filter(|record| record.role == fs3_core::TurnRole::Agent)
        .count();

    assert_eq!(assistant_records, 38, "the fixture's own arithmetic");
    assert_eq!(
        u64::try_from(merged).expect("a small count"),
        distinct,
        "claude writes one line per content BLOCK, so {assistant_records} assistant records \
         are {distinct} turns; a reader that emits one turn per record reports a \
         conversation that never happened"
    );
}

/// The mutation check: this fails if the merge is ever changed to an
/// adjacent-run fold.
///
/// Message `msg_011CeU5WTE6uaCEikVXuAQhT` is written as thinking / text /
/// tool_use, then INTERRUPTED by the `user` record carrying its tool result,
/// then continued with a second tool_use. Collapsing adjacent runs would split
/// it in two — 20 groups instead of 13 across this file — and every count-only
/// assertion would still pass. So this pins the shape, not the count.
#[test]
fn a_message_interrupted_by_its_own_tool_result_stays_one_turn() {
    let (_scratch, root) = scratch_store("interrupted");
    let records = read_all(&ClaudeSource::new(&root), &native(MAIN));

    let first_block = "9ccf07af-ba99-4554-bfb4-b02591244f76";
    let continuation = "3bf9025b-b99f-4570-beba-7f0ffdb6bf74";

    let merged = records
        .iter()
        .find(|record| record.ordinal == first_block)
        .expect("the merged turn is reported under its FIRST block's uuid");

    let calls = merged
        .items
        .iter()
        .filter(|item| matches!(item, TurnItem::ToolCall { .. }))
        .count();
    assert_eq!(
        calls, 2,
        "both tool calls of this message belong to ONE turn — the second arrives after the \
         user record that answers the first, so an adjacent-run fold would find only one"
    );

    assert!(
        !records.iter().any(|record| record.ordinal == continuation),
        "the continuation block is folded into its message and must not surface as a turn \
         of its own"
    );
}

#[test]
fn a_merged_turn_is_reported_under_its_first_block_not_its_last() {
    let (_scratch, root) = scratch_store("ordinal");
    let records = read_all(&ClaudeSource::new(&root), &native(MAIN));

    assert!(
        records
            .iter()
            .any(|record| record.ordinal == "9ccf07af-ba99-4554-bfb4-b02591244f76"),
        "first uuid of the group: it is stable under a rescan, where the last uuid changes \
         as the group grows and would defeat the dedupe"
    );
    assert!(
        !records
            .iter()
            .any(|record| record.ordinal == "82ab2abe-25cd-4331-aa98-b0d4031948f5"),
        "the group's later blocks are not ordinals of their own"
    );
}

// ------------------------------------------------------------- allowlist

#[test]
fn an_unknown_record_type_is_dropped_and_its_neighbours_still_parse() {
    let (_scratch, root) = scratch_store("unknown");
    let session = "11111111-2222-3333-4444-555555555555";
    let path = root.join(format!("{session}.jsonl"));

    // A type no allowlist has heard of, between two ordinary user turns.
    append(
        &path,
        concat!(
            r#"{"type":"user","uuid":"u-1","timestamp":"2026-08-28T00:00:00.000Z","#,
            r#""message":{"role":"user","content":"before"}}"#,
            "\n",
            r#"{"type":"quantum-entanglement-latch","uuid":"x-1","#,
            r#""timestamp":"2026-08-28T00:00:01.000Z","payload":{"whatever":true}}"#,
            "\n",
            r#"{"type":"user","uuid":"u-2","timestamp":"2026-08-28T00:00:02.000Z","#,
            r#""message":{"role":"user","content":"after"}}"#,
            "\n",
        ),
    );

    let source = ClaudeSource::new(&root);
    let records = read_all(&source, &native(session));

    let ordinals: Vec<&str> = records.iter().map(|r| r.ordinal.as_str()).collect();
    assert_eq!(
        ordinals,
        vec!["u-1", "u-2"],
        "an unfamiliar record type is a DROP, never an error and never a panic — an ingest \
         that dies because the store grew a bookkeeping row is worse than one that ignores it"
    );
}

#[test]
fn store_metadata_rows_are_not_turns() {
    let (_scratch, root) = scratch_store("metadata");
    let records = read_all(&ClaudeSource::new(&root), &native(MAIN));

    // The store holds 148 records; only user and assistant bear turns.
    assert_eq!(
        records.len(),
        39,
        "13 merged assistant turns + 26 user turns"
    );
    assert!(
        records.len() < 148,
        "attachment, mode, pr-link and friends describe the session rather than anything \
         said in it"
    );
}

// ----------------------------------------------------------------- spill

#[test]
fn a_spilled_tool_result_is_resolved_to_its_full_bytes() {
    let (_scratch, root) = scratch_store("spill");
    let records = read_all(&ClaudeSource::new(&root), &native(SPILLED));

    let spill = fixtures_root()
        .join("claude")
        .join(SPILLED)
        .join("tool-results")
        .join("b8e9hq4my.txt");
    let expected = std::fs::read_to_string(&spill).expect("the committed spill");

    let resolved = records
        .iter()
        .flat_map(|record| record.items.iter())
        .find_map(|item| match item {
            TurnItem::ToolResult {
                head, total_bytes, ..
            } if *total_bytes == expected.len() as u64 => Some(head),
            _ => None,
        })
        .expect("the spilled result is followed, not left as a file name");

    assert_eq!(
        resolved, &expected,
        "the record keeps only a ~2KB preview; the body lives in a sibling file"
    );
    assert!(
        !resolved.contains("b8e9hq4my.txt\n\nPreview"),
        "a turn whose body is a file name is exactly what gotcha 9 forbids"
    );
}

#[test]
fn an_unresolvable_spill_falls_back_to_the_preview_and_says_it_is_short() {
    let (_scratch, root) = scratch_store("spill-missing");
    // The machine that wrote the record is not the machine reading it, and the
    // spill directory may simply not have travelled.
    std::fs::remove_dir_all(root.join(SPILLED).join("tool-results")).expect("drop the spill");

    let records = read_all(&ClaudeSource::new(&root), &native(SPILLED));
    let truncated = records
        .iter()
        .flat_map(|record| record.items.iter())
        .any(|item| {
            matches!(
                item,
                TurnItem::ToolResult {
                    truncated: true,
                    ..
                }
            )
        });

    assert!(
        truncated,
        "a tool result that cannot be read is a smaller result, not a failed ingest — but it \
         must not claim to be whole"
    );
}

// ---------------------------------------------------------- expectations

#[test]
fn the_committed_fixtures_are_unchanged() {
    Expectations::load(FixtureStore::Claude).verify_fixtures_unchanged();
}

#[test]
fn emitted_ordinals_are_a_subsequence_of_what_the_store_holds() {
    let (_scratch, root) = scratch_store("subsequence");
    let source = ClaudeSource::new(&root);
    let expectations = Expectations::load(FixtureStore::Claude);

    for key in [MAIN, SPILLED] {
        // The expectations walk main-then-children, which is the order resolve
        // returns and therefore the order a reader consumes them in.
        let files = source.resolve(&native(key)).expect("resolve");
        let mut observed = Vec::new();
        for file in &files {
            let batch = source.read_incremental(file, None).expect("a full read");
            observed.extend(batch.records.into_iter().map(|record| record.ordinal));
        }
        expectations.assert_ordinals_are_a_subsequence(key, &observed);
    }
}

// -------------------------------------------------------------- framing

#[test]
fn a_foreign_cursor_is_refused() {
    let (_scratch, root) = scratch_store("foreign");
    let source = ClaudeSource::new(&root);
    let files = source.resolve(&native(MAIN)).expect("resolve");

    let outcome = source.read_incremental(&files[0], Some(&SourceCursor::Seq { seq: 0 }));

    assert!(
        outcome.is_err(),
        "read as zero, a cursor from another store would silently re-ingest an entire \
         conversation"
    );
}
