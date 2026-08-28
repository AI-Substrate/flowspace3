//! The shared contract and the committed expectations, over the omp and pij
//! fixtures (plan 005, unit u1b).
//!
//! # The fixtures grow with REAL bytes
//!
//! The contract suite writes on purpose — case 4 appends, case 5 tears a line —
//! so everything here runs over a SCRATCH COPY and the committed fixtures stay
//! byte-identical (their sha256 is asserted separately by
//! `Expectations::verify_fixtures_unchanged`).
//!
//! What the scratch copy contains is the part worth stating: a real PREFIX of
//! the committed file, with `grow()` appending the real REMAINING LINES and one
//! final real line held back to be torn in half. Nothing is synthesised. A
//! fixture that grew by something its store would never write would prove the
//! suite, not the reader — and the counts are computed FROM THE BYTES rather
//! than hand-written, so regenerating a fixture cannot silently invalidate them.
//!
//! The omp prefix deliberately ends just past the COMPACTION SEAM, putting the
//! seam on the read boundary rather than safely inside one side of it.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use fs3_core::{
    ConversationSource, Harness, IngestInput, RawRecord, SessionFile, SourceCursor, TurnItem,
};
use fs3_providers::conversation_sources::{omp::OmpSource, pij_ledger::PijLedgerSource};
use fs3_testkit::{
    Expectations, FixtureStore, SourceFixture, conversation_source_contract, fixtures_root,
};

/// The omp conversation the fixtures hold.
const OMP_SESSION: &str = "01a03d08-7c56-7000-ac9b-95c4b3ef34d7";
/// The omp session file's own name.
const OMP_FILE: &str = "2026-08-26T07-46-01-430Z_01a03d08-7c56-7000-ac9b-95c4b3ef34d7.jsonl";
/// The seat whose ledger the fixtures hold.
const PIJ_SEAT: &str = "pij-linguistic-narwhal";

/// Lines of the omp fixture seeded before the contract suite runs.
///
/// 186 puts the boundary immediately after the compaction record (line 184) and
/// its injected continuation turn (line 185).
const OMP_PREFIX: usize = 186;
/// Lines of the pij fixture seeded before the contract suite runs.
///
/// 40 keeps BOTH receipts (lines 4 and 9) inside the first read.
const PIJ_PREFIX: usize = 40;

// ---------------------------------------------------------------- scratch

/// A private copy of a committed fixture, seeded short so it can grow.
struct Scratch {
    root: PathBuf,
    file: PathBuf,
    /// Lines not yet appended; the last is reserved for the torn-record case.
    pending: Vec<String>,
}

impl Scratch {
    /// Copy `source`'s first `prefix` lines to `file`, holding the rest back.
    fn new(root: PathBuf, source: &Path, file: PathBuf, prefix: usize) -> Self {
        let all: Vec<String> = std::fs::read_to_string(source)
            .expect("a committed fixture must be readable")
            .lines()
            .map(str::to_owned)
            .collect();
        assert!(
            prefix < all.len(),
            "the prefix must leave real lines to grow by"
        );
        std::fs::create_dir_all(file.parent().expect("a fixture file has a parent"))
            .expect("scratch directories must be creatable");
        write_lines(&file, &all[..prefix]);
        Self {
            root,
            file,
            pending: all[prefix..].to_vec(),
        }
    }

    /// Append every pending line except the one reserved for tearing.
    fn append_all_but_the_last(&mut self) -> Vec<String> {
        let appended: Vec<String> = self.pending.drain(..self.pending.len() - 1).collect();
        let mut text = std::fs::read_to_string(&self.file).expect("the scratch file must exist");
        for line in &appended {
            text.push_str(line);
            text.push('\n');
        }
        std::fs::write(&self.file, text).expect("the scratch file must be writable");
        appended
    }

    /// Write the first half of the reserved line, with no terminator.
    fn tear(&self) -> bool {
        let line = self
            .pending
            .last()
            .expect("a line was reserved for tearing");
        let half = line.floor_char_boundary(line.len() / 2);
        let mut text = std::fs::read_to_string(&self.file).expect("the scratch file must exist");
        text.push_str(&line[..half]);
        std::fs::write(&self.file, text).expect("the scratch file must be writable");
        true
    }

    /// Finish the torn line.
    fn mend(&self) {
        let line = self
            .pending
            .last()
            .expect("a line was reserved for tearing");
        let half = line.floor_char_boundary(line.len() / 2);
        let mut text = std::fs::read_to_string(&self.file).expect("the scratch file must exist");
        text.push_str(&line[half..]);
        text.push('\n');
        std::fs::write(&self.file, text).expect("the scratch file must be writable");
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn write_lines(path: &Path, lines: &[String]) {
    let mut text = String::new();
    for line in lines {
        text.push_str(line);
        text.push('\n');
    }
    std::fs::write(path, text).expect("the scratch file must be writable");
}

/// A scratch root no other test can be handed.
///
/// Follows testkit's own precedent (`fake_source.rs`) rather than adding a
/// `tempfile` dependency — but ALL of it, which is the part that bit us.
///
/// `SystemTime::now()` alone is NOT unique. Cargo runs these tests on parallel
/// threads, and two that start together can read the same nanosecond, so two
/// fixtures sharing a label got the SAME directory — and whichever `Scratch`
/// dropped first ran `remove_dir_all` on the other's live file. That failed
/// about one run in three, on a DIFFERENT test each time, always as
/// `cannot open: No such file or directory` mid-test. A single green run
/// cannot detect it, which is how it reached the composed tree.
///
/// The process-static counter makes a collision structurally impossible within
/// a process; the pid makes it impossible between concurrent test binaries too.
fn scratch_root(label: &str) -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("the clock must be after the epoch")
        .as_nanos();
    let unique = NEXT.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let root = std::env::temp_dir().join(format!("fs3-{label}-{pid}-{nanos}-{unique}"));
    std::fs::create_dir_all(&root).expect("a scratch root must be creatable");
    root
}

// ------------------------------------------------------- counting the bytes

/// Records the omp reader emits from these lines.
///
/// An INDEPENDENT restatement of the ruled allowlist, evaluated over the bytes:
/// `message`, `compaction` and `custom_message`, each of which must carry an
/// `id` to have an ordinal at all. Deriving it here rather than hand-counting
/// is what keeps the expectations honest when a fixture is regenerated.
fn omp_emitted(lines: &[String]) -> usize {
    lines
        .iter()
        .filter(|line| {
            let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
                return false;
            };
            let kind = value.get("type").and_then(serde_json::Value::as_str);
            matches!(kind, Some("message" | "compaction" | "custom_message"))
                && value
                    .get("id")
                    .and_then(serde_json::Value::as_str)
                    .is_some()
        })
        .count()
}

/// Records the pij reader emits from these lines.
fn pij_emitted(lines: &[String]) -> usize {
    lines
        .iter()
        .filter(|line| {
            let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
                return false;
            };
            let kind = value.get("type").and_then(serde_json::Value::as_str);
            matches!(
                kind,
                Some("message" | "tool_call" | "tool_result" | "receipt")
            ) && value
                .get("seq")
                .and_then(serde_json::Value::as_u64)
                .is_some()
        })
        .count()
}

fn committed_lines(relative: &str) -> Vec<String> {
    std::fs::read_to_string(fixtures_root().join(relative))
        .expect("a committed fixture must be readable")
        .lines()
        .map(str::to_owned)
        .collect()
}

// -------------------------------------------------------------- omp fixture

struct OmpFixture {
    source: OmpSource,
    scratch: Scratch,
    folder: PathBuf,
    seeded: usize,
    grown: usize,
}

impl OmpFixture {
    fn new(prefix: usize) -> Self {
        let root = scratch_root("omp-source");
        // home/<workspace> so the reader's slug rule has something to strip.
        let folder = root.join("substrate/flowspace/flowspace3");
        let sessions = root.join(".omp/agent/sessions/-substrate-flowspace-flowspace3");
        let committed = fixtures_root().join("omp").join(OMP_FILE);

        let scratch = Scratch::new(root.clone(), &committed, sessions.join(OMP_FILE), prefix);

        // The spill directory travels with the session: resolving
        // `artifact://30` is part of the reader's job, not a special case.
        let spill_src = fixtures_root()
            .join("omp")
            .join(OMP_FILE.trim_end_matches(".jsonl"));
        if spill_src.is_dir() {
            let spill_dst = sessions.join(OMP_FILE.trim_end_matches(".jsonl"));
            std::fs::create_dir_all(&spill_dst).expect("the spill directory must be creatable");
            for entry in
                std::fs::read_dir(&spill_src).expect("the spill directory must be readable")
            {
                let entry = entry.expect("a spill entry must be readable");
                std::fs::copy(entry.path(), spill_dst.join(entry.file_name()))
                    .expect("a spill file must be copyable");
            }
        }

        let seeded = omp_emitted(&committed_lines(&format!("omp/{OMP_FILE}"))[..prefix]);
        Self {
            source: OmpSource::new(root.join(".omp/agent/sessions"), root),
            scratch,
            folder,
            seeded,
            grown: 0,
        }
    }
}

impl SourceFixture for OmpFixture {
    fn source(&self) -> &dyn ConversationSource {
        &self.source
    }

    fn input(&self) -> IngestInput {
        IngestInput::Native {
            session_id: OMP_SESSION.to_owned(),
            harness: Harness::Omp,
            folder: self.folder.clone(),
        }
    }

    fn expected_session_files(&self) -> usize {
        // One. The `<session>/` directory beside the file holds spilled tool
        // OUTPUT, which is a payload, not a conversation.
        1
    }

    fn expected_records(&self) -> usize {
        self.seeded + self.grown
    }

    fn grow(&mut self) -> usize {
        let appended = self.scratch.append_all_but_the_last();
        let emitted = omp_emitted(&appended);
        self.grown += emitted;
        emitted
    }

    fn begin_partial_record(&mut self) -> bool {
        self.scratch.tear()
    }

    fn finish_partial_record(&mut self) {
        self.scratch.mend();
    }
}

// -------------------------------------------------------------- pij fixture

struct PijFixture {
    source: PijLedgerSource,
    scratch: Scratch,
    folder: PathBuf,
    seeded: usize,
    grown: usize,
}

impl PijFixture {
    fn new(prefix: usize) -> Self {
        let root = scratch_root("pij-source");
        let committed = fixtures_root().join("pij/events.ndjson");
        let ledger = root.join(".pij").join(PIJ_SEAT).join("events.ndjson");
        let scratch = Scratch::new(root.clone(), &committed, ledger, prefix);
        let seeded = pij_emitted(&committed_lines("pij/events.ndjson")[..prefix]);
        Self {
            source: PijLedgerSource::new(root.join(".pij")),
            scratch,
            folder: root,
            seeded,
            grown: 0,
        }
    }
}

impl SourceFixture for PijFixture {
    fn source(&self) -> &dyn ConversationSource {
        &self.source
    }

    fn input(&self) -> IngestInput {
        IngestInput::Pij {
            id: PIJ_SEAT.to_owned(),
            folder: self.folder.clone(),
        }
    }

    fn expected_session_files(&self) -> usize {
        // A spawned peer gets its own seat, so a ledger has no children.
        1
    }

    fn expected_records(&self) -> usize {
        self.seeded + self.grown
    }

    fn grow(&mut self) -> usize {
        let appended = self.scratch.append_all_but_the_last();
        let emitted = pij_emitted(&appended);
        self.grown += emitted;
        emitted
    }

    fn begin_partial_record(&mut self) -> bool {
        self.scratch.tear()
    }

    fn finish_partial_record(&mut self) {
        self.scratch.mend();
    }
}

// ------------------------------------------------------------- the contract

#[test]
fn the_omp_reader_satisfies_the_contract() {
    conversation_source_contract(&mut OmpFixture::new(OMP_PREFIX));
}

#[test]
fn the_pij_ledger_reader_satisfies_the_contract() {
    conversation_source_contract(&mut PijFixture::new(PIJ_PREFIX));
}

// --------------------------------------------------- the committed expectations

/// Every record of a whole fixture, read through the reader under test.
fn read_everything(fixture: &dyn SourceFixture) -> Vec<RawRecord> {
    let files = fixture
        .source()
        .resolve(&fixture.input())
        .expect("resolve must find the conversation");
    let main = files.first().expect("a conversation has a main file");
    fixture
        .source()
        .read_incremental(main, None)
        .expect("a full read must succeed")
        .records
}

/// A fixture seeded with the WHOLE committed file, for the expectation claims.
fn omp_whole() -> OmpFixture {
    OmpFixture::new(committed_lines(&format!("omp/{OMP_FILE}")).len() - 1)
}

fn pij_whole() -> PijFixture {
    PijFixture::new(committed_lines("pij/events.ndjson").len() - 1)
}

#[test]
fn the_committed_omp_fixture_is_unchanged() {
    Expectations::load(FixtureStore::Omp).verify_fixtures_unchanged();
}

#[test]
fn the_committed_pij_fixture_is_unchanged() {
    Expectations::load(FixtureStore::Pij).verify_fixtures_unchanged();
}

#[test]
fn omp_ordinals_are_a_subsequence_of_the_store() {
    let mut fixture = omp_whole();
    fixture.grow();
    let ordinals: Vec<String> = read_everything(&fixture)
        .into_iter()
        .map(|record| record.ordinal)
        .collect();
    Expectations::load(FixtureStore::Omp).assert_ordinals_are_a_subsequence(OMP_SESSION, &ordinals);
}

/// The CARDINALITY claim, over the COMMITTED bytes rather than a scratch
/// fixture.
///
/// It reads the committed file directly and deliberately: `omp_whole()` grows
/// a scratch copy and the `SourceFixture` contract holds one line back for the
/// torn-record case, so a grown fixture is legitimately one record short of the
/// store. The emitted expectation is a statement about what the STORE implies,
/// so it is asserted against the store's own bytes.
#[test]
fn the_omp_reader_emits_exactly_what_the_committed_store_implies() {
    let root = fs3_testkit::expectations::fixtures_root().join("omp");
    let source = OmpSource::new(&root, root.clone());
    let files = source
        .resolve(&IngestInput::Native {
            session_id: OMP_SESSION.to_string(),
            harness: Harness::Omp,
            folder: root.clone(),
        })
        .expect("resolve must find the committed conversation");
    let ordinals: Vec<String> = source
        .read_incremental(&files[0], None)
        .expect("a full read of the committed bytes")
        .records
        .into_iter()
        .map(|record| record.ordinal)
        .collect();
    Expectations::load(FixtureStore::Omp).assert_emitted_ordinals_match(OMP_SESSION, &ordinals);
}

#[test]
fn pij_ordinals_are_a_subsequence_of_the_store() {
    let mut fixture = pij_whole();
    fixture.grow();
    let ordinals: Vec<String> = read_everything(&fixture)
        .into_iter()
        .map(|record| record.ordinal)
        .collect();
    Expectations::load(FixtureStore::Pij).assert_ordinals_are_a_subsequence(PIJ_SEAT, &ordinals);
}

/// The CARDINALITY claim for the ledger, over the COMMITTED bytes — see the omp
/// test above for why a grown fixture is the wrong corpus for it.
#[test]
fn the_pij_reader_emits_exactly_what_the_committed_store_implies() {
    // The ledger store is `<root>/<seat>/events.ndjson`, and the fixture is
    // committed flat, so the seat directory is materialised around a COPY of
    // the committed bytes. The claim is still about those bytes.
    let root = scratch_root("pij-committed");
    let seat_dir = root.join(PIJ_SEAT);
    std::fs::create_dir_all(&seat_dir).expect("a seat directory must be creatable");
    let committed = fs3_testkit::expectations::fixtures_root().join("pij/events.ndjson");
    std::fs::copy(&committed, seat_dir.join("events.ndjson")).expect("copy the committed ledger");

    let source = PijLedgerSource::new(&root);
    let files = source
        .resolve(&IngestInput::Pij {
            id: PIJ_SEAT.to_string(),
            folder: root.clone(),
        })
        .expect("resolve must find the committed ledger");
    let ordinals: Vec<String> = source
        .read_incremental(&files[0], None)
        .expect("a full read of the committed bytes")
        .records
        .into_iter()
        .map(|record| record.ordinal)
        .collect();
    Expectations::load(FixtureStore::Pij).assert_emitted_ordinals_match(PIJ_SEAT, &ordinals);
}

#[test]
fn every_omp_oracle_prose_turn_appears() {
    let mut fixture = omp_whole();
    fixture.grow();
    let bodies: Vec<String> = read_everything(&fixture)
        .into_iter()
        .map(|record| record.body)
        .collect();
    Expectations::load(FixtureStore::Omp).assert_oracle_prose_appears(OMP_SESSION, &bodies);
}

#[test]
fn every_pij_oracle_prose_turn_appears() {
    // Deliberately weak, and labelled weak: the oracle yields 3 turns from 50
    // records and only ONE of them is a prose kind, because `read_pij_ledger`
    // keeps only role user/assistant with a text block and this window is
    // tool-heavy. A green result here is not evidence the reader is right —
    // the structural claim above is this store's real done-bar.
    let mut fixture = pij_whole();
    fixture.grow();
    let bodies: Vec<String> = read_everything(&fixture)
        .into_iter()
        .map(|record| record.body)
        .collect();
    Expectations::load(FixtureStore::Pij).assert_oracle_prose_appears(PIJ_SEAT, &bodies);
}

// ------------------------------------------------------------ named claims

#[test]
fn the_compaction_record_is_never_dropped() {
    // ac-0005, and the single most likely thing to be silently lost: the
    // reference oracle drops it, handling only `type == "message"`.
    let mut fixture = omp_whole();
    fixture.grow();
    let records = read_everything(&fixture);
    let compaction = records
        .iter()
        .find(|record| record.ordinal == "a932507b")
        .expect("the compaction record must be emitted (ac-0005)");
    assert!(
        compaction.body.starts_with("## Goal"),
        "the compaction turn carries its summary: {:?}",
        compaction.body
    );
    // It sits IN the parent chain: the injected continuation turn names it.
    assert!(
        records
            .iter()
            .any(|record| record.parent_ordinal.as_deref() == Some("a932507b")),
        "dropping compaction would also break the parent chain across the seam"
    );
}

#[test]
fn every_tool_result_pairs_with_exactly_one_call() {
    // u3, proven over the committed bytes rather than asserted: omp names a
    // tool under `name` on the CALL and `toolName` only on the RESULT, and a
    // `tool_execution_start` mirror precedes every call — so a naive count
    // doubles. Zero orphans in either direction is the property that says the
    // pairing key is right.
    let mut calls = std::collections::BTreeSet::new();
    let mut results = std::collections::BTreeSet::new();
    for line in committed_lines(&format!("omp/{OMP_FILE}")) {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        if value.get("type").and_then(serde_json::Value::as_str) != Some("message") {
            continue;
        }
        let message = &value["message"];
        match message.get("role").and_then(serde_json::Value::as_str) {
            Some("assistant") => {
                for block in message["content"].as_array().into_iter().flatten() {
                    if block.get("type").and_then(serde_json::Value::as_str) == Some("toolCall")
                        && let Some(id) = block.get("id").and_then(serde_json::Value::as_str)
                    {
                        calls.insert(id.to_owned());
                    }
                }
            }
            Some("toolResult") => {
                if let Some(id) = message
                    .get("toolCallId")
                    .and_then(serde_json::Value::as_str)
                {
                    results.insert(id.to_owned());
                }
            }
            _ => {}
        }
    }
    assert_eq!(calls.len(), 72, "the window holds 72 tool calls");
    assert_eq!(results.len(), 72, "the window holds 72 tool results");
    assert!(
        calls.difference(&results).next().is_none(),
        "a call with no result: {:?}",
        calls.difference(&results).collect::<Vec<_>>()
    );
    assert!(
        results.difference(&calls).next().is_none(),
        "a result with no call: {:?}",
        results.difference(&calls).collect::<Vec<_>>()
    );
}

#[test]
fn an_xd_tool_call_is_never_reported_as_a_file_operation() {
    // Recipe gotcha 2, keyed on the observable property. Five calls in this
    // window carry an `xd://` path: four `write` and one `read`.
    let mut fixture = omp_whole();
    fixture.grow();
    let mut remapped = 0;
    for record in read_everything(&fixture) {
        for item in &record.items {
            if let TurnItem::ToolCall { tool, input } = item {
                let fs3_core::ToolInput::Verbatim { text } = input else {
                    panic!("a reader emits verbatim input; intake applies the payload policy");
                };
                if text.contains("xd://") {
                    remapped += 1;
                    assert_eq!(
                        tool, "pij_send",
                        "an xd:// call must be attributed to the tool it invokes, never to \
                         the file verb the store spelled it with"
                    );
                }
            }
        }
    }
    assert_eq!(
        remapped, 5,
        "four `write` plus one `read` carry an xd:// path in this window; a name-keyed \
         rule finds only four"
    );
}

#[test]
fn a_spilled_tool_result_is_resolved_from_its_artifact_file() {
    // Measured: the inline body is NOT a prefix of the spill file. It
    // abbreviates the git sha to seven characters and omits the `Author:` line
    // entirely, so a 512-byte head of each is different text. Asserting the
    // BEHAVIOUR — the body came from the file — rather than a byte-exact total,
    // because the committed spill is itself sanitiser-capped and a size claim
    // would be false of these bytes.
    let mut fixture = omp_whole();
    fixture.grow();
    let records = read_everything(&fixture);
    let spilled = records
        .iter()
        .find(|record| record.ordinal == "63744cee")
        .expect("the spilled tool result must be emitted");
    let [TurnItem::ToolResult { head, .. }] = &spilled.items[..] else {
        panic!("expected exactly one tool result: {:?}", spilled.items);
    };
    assert!(
        head.starts_with("commit 7975adc405f09448d942831326477f6635f0fbc8"),
        "the resolved body must carry the FULL forty-character sha, not the inline \
         body's seven: {:?}",
        &head[..head.len().min(80)]
    );
    assert!(
        head.contains("Author:"),
        "the inline body has no `Author:` line at all; its presence is what proves the \
         body was resolved from the artifact file rather than read off the record"
    );
}

#[test]
fn a_foreign_cursor_is_refused_by_both_readers() {
    // u6, both directions. Read as zero, either would silently re-ingest an
    // entire conversation.
    let omp = OmpFixture::new(OMP_PREFIX);
    let omp_files = omp.source.resolve(&omp.input()).expect("resolve");
    assert!(
        omp.source
            .read_incremental(&omp_files[0], Some(&SourceCursor::Seq { seq: 0 }))
            .is_err(),
        "the omp reader must refuse a sequence cursor"
    );

    let pij = PijFixture::new(PIJ_PREFIX);
    let pij_files = pij.source.resolve(&pij.input()).expect("resolve");
    assert!(
        pij.source
            .read_incremental(
                &pij_files[0],
                Some(&SourceCursor::ByteOffset {
                    device: 0,
                    inode: 0,
                    offset: 0,
                }),
            )
            .is_err(),
        "the pij ledger reader must refuse a byte-offset cursor"
    );
}

#[test]
fn resolve_stamps_every_file_with_its_own_store() {
    let omp = OmpFixture::new(OMP_PREFIX);
    let files: Vec<SessionFile> = omp.source.resolve(&omp.input()).expect("resolve");
    assert!(files.iter().all(|file| file.harness == Harness::Omp));

    let pij = PijFixture::new(PIJ_PREFIX);
    let files: Vec<SessionFile> = pij.source.resolve(&pij.input()).expect("resolve");
    assert!(files.iter().all(|file| file.harness == Harness::PijLedger));
}
