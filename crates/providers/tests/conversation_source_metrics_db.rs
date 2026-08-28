//! The metrics-db reader against its committed fixtures (plan 005, u1d).
//!
//! Four claims, in the order they are worth making:
//!
//! 1. the shared [`conversation_source_contract`] — the mechanical done-bar
//!    every reader passes or is not finished;
//! 2. the committed structural expectation — emitted ordinals are an in-order,
//!    repeat-free subsequence of the ids the store holds — and the oracle's
//!    prose, verbatim;
//! 3. repo scoping, proven twice: by exclusion over the foreign-repo rows this
//!    fixture deliberately carries, and by API shape;
//! 4. the copilot dialect, which no oracle covers, and the unknown-event drop.
//!
//! Everything runs over a SCRATCH COPY in a temp directory. The committed
//! fixture bytes are pinned by sha256 and asserted unchanged on every run —
//! contract cases write on purpose, so writing to the committed database would
//! break every other reader's expectations too.

use std::path::PathBuf;

use fs3_core::{
    ConversationSource, Harness, IngestInput, SessionKind, SourceCursor, TurnItem, TurnRole,
    TurnSource,
};
use fs3_providers::conversation_sources::metrics_db::{MetricsDbSource, RepoScope};
use fs3_testkit::{
    Expectations, FixtureStore, SourceFixture, conversation_source_contract, fixtures_root,
};
use rusqlite::Connection;

/// The repository this fixture's real rows belong to.
const FLOWSPACE3: &str = "https://github.com/AI-Substrate/flowspace3";
/// The repository its negative rows belong to.
const FOREIGN: &str = "https://github.com/AI-Substrate/pij";

const MAIN: &str = "a5a5588f-0979-439f-a1bf-ddf185a089c7";
const SUBAGENT: &str = "agent-a01869bcb5e09448b";
const COPILOT: &str = "222c2c9d-5798-48cf-9dbd-cd4a52324c53";

/// A writable copy of the committed database, deleted when it drops.
///
/// `std::env::temp_dir` and a unique name rather than a `tempfile` dependency:
/// this unit's one dependency edge is spent on `rusqlite`, and the repo's own
/// testkit already does exactly this.
struct Scratch {
    directory: PathBuf,
}

impl Scratch {
    fn new(label: &str) -> Self {
        let unique = format!(
            "fs3-metrics-db-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock is after the epoch")
                .as_nanos()
        );
        let directory = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&directory).expect("scratch directory");

        let source = fixtures_root().join("metrics_db/metrics.sqlite3");
        let database = directory.join("metrics.sqlite3");
        std::fs::copy(&source, &database).expect("copy the fixture database");
        Self { directory }
    }

    fn database(&self) -> PathBuf {
        self.directory.join("metrics.sqlite3")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

/// The reader plus its scratch database, as the contract suite needs them.
struct Fixture {
    scratch: Scratch,
    source: MetricsDbSource,
    /// Rows already copied in by `grow`, so a second call cannot collide.
    grown: i64,
}

impl Fixture {
    fn new(label: &str) -> Self {
        let scratch = Scratch::new(label);
        let source = MetricsDbSource::new(scratch.database(), RepoScope::remote_url(FLOWSPACE3));
        Self {
            scratch,
            source,
            grown: 0,
        }
    }
}

impl SourceFixture for Fixture {
    fn source(&self) -> &dyn ConversationSource {
        &self.source
    }

    fn input(&self) -> IngestInput {
        IngestInput::Native {
            session_id: MAIN.to_owned(),
            harness: Harness::MetricsDb,
            folder: PathBuf::from("/Users/agent/substrate/flowspace/flowspace3"),
        }
    }

    fn expected_session_files(&self) -> usize {
        // The main conversation and the one subagent linked to it by
        // `external_parent_session_id`.
        2
    }

    fn expected_records(&self) -> usize {
        // 56 rows − 34 bookkeeping = 22 candidates = 9 user rows (never merged)
        // + 13 assistant rows folding into 7 `message.id` groups.
        16
    }

    fn grow(&mut self) -> usize {
        // REAL rows, copied verbatim out of this store: two `user` records that
        // carry no `message.id`, so they cannot fold into an existing group and
        // each is unambiguously one new turn. Inventing a record shape the store
        // would never write would prove the suite, not the reader.
        const TEMPLATES: [i64; 2] = [945_061, 945_066];

        let connection = Connection::open(self.scratch.database()).expect("open scratch");
        let highest: i64 = connection
            .query_row("select max(rowid) from metrics", [], |row| row.get(0))
            .expect("max rowid");

        for (offset, template) in TEMPLATES.iter().enumerate() {
            let new_id = highest + self.grown + offset as i64 + 1;
            connection
                .execute(
                    "insert into metrics (id, event_json, attempts, next_retry_at, event_ts, \
                     event_kind, tool, external_session_id) \
                     select ?1, event_json, 0, 0, event_ts, event_kind, tool, external_session_id \
                     from metrics where rowid = ?2",
                    (new_id, template),
                )
                .expect("copy a real row forward");
        }

        self.grown += TEMPLATES.len() as i64;
        TEMPLATES.len()
    }

    fn begin_partial_record(&mut self) -> bool {
        // A sqlite row is committed or absent — there is no torn state to read.
        // The trait licenses this explicitly, and faking one would be coverage
        // that proves nothing.
        false
    }

    fn finish_partial_record(&mut self) {
        unreachable!("begin_partial_record returned false");
    }
}

#[test]
fn the_reader_satisfies_the_shared_contract() {
    let mut fixture = Fixture::new("contract");
    conversation_source_contract(&mut fixture);
}

#[test]
fn the_committed_fixtures_are_unchanged() {
    Expectations::load(FixtureStore::MetricsDb).verify_fixtures_unchanged();
}

#[test]
fn emitted_ordinals_are_a_subsequence_of_what_the_store_holds() {
    let expectations = Expectations::load(FixtureStore::MetricsDb);
    let fixture = Fixture::new("subsequence");

    for session in [MAIN, SUBAGENT] {
        let file = session_file(&fixture, session);
        let batch = fixture
            .source
            .read_incremental(&file, None)
            .expect("read the whole session");
        let ordinals: Vec<String> = batch
            .records
            .iter()
            .map(|record| record.ordinal.clone())
            .collect();

        expectations.assert_ordinals_are_a_subsequence(session, &ordinals);
    }
}

#[test]
fn the_two_sessions_yield_the_records_the_merge_arithmetic_predicts() {
    let fixture = Fixture::new("counts");

    let main = read_all(&fixture, MAIN);
    assert_eq!(main.len(), 16, "9 unmerged user rows + 7 message.id groups");

    let subagent = read_all(&fixture, SUBAGENT);
    assert_eq!(
        subagent.len(),
        10,
        "5 unmerged user rows + 5 message.id groups"
    );

    // The merge is the point: 13 assistant rows became 7 turns, so a reader that
    // stopped merging would still pass a naive "some records came back" check.
    let agent_turns = main
        .iter()
        .filter(|record| record.role == TurnRole::Agent)
        .count();
    assert_eq!(agent_turns, 7, "13 assistant rows fold into 7 turns");
}

#[test]
fn the_oracle_prose_appears_verbatim_and_in_order() {
    let expectations = Expectations::load(FixtureStore::MetricsDb);
    let fixture = Fixture::new("oracle");

    let bodies: Vec<String> = read_all(&fixture, MAIN)
        .into_iter()
        .map(|record| record.body)
        .collect();

    expectations.assert_oracle_prose_appears(MAIN, &bodies);
}

#[test]
fn a_compaction_summary_is_kept_and_marked_as_written_by_the_harness() {
    let fixture = Fixture::new("compaction");
    let compaction = read_all(&fixture, MAIN)
        .into_iter()
        .find(|record| record.ordinal == "945255")
        .expect("the compaction record is never dropped — it is the only record of lost context");

    assert_eq!(compaction.source, TurnSource::System);
    assert_eq!(compaction.role, TurnRole::Human);
    assert!(
        compaction
            .body
            .starts_with("This session is being continued from a previous conversation")
    );
}

#[test]
fn an_injected_peer_packet_is_not_reported_as_a_human_turn() {
    let fixture = Fixture::new("peer");
    let injected = read_all(&fixture, MAIN)
        .into_iter()
        .find(|record| record.ordinal == "945089")
        .expect("the pij packet row");

    // Role-only would report an orchestrated fleet as half-human (workshop 005).
    assert_eq!(injected.role, TurnRole::Human);
    assert_eq!(injected.source, TurnSource::Peer);
}

// --- repo scoping -------------------------------------------------------

#[test]
fn a_foreign_repo_session_is_invisible_to_a_scoped_reader() {
    let scratch = Scratch::new("scope-exclusion");
    let ours = MetricsDbSource::new(scratch.database(), RepoScope::remote_url(FLOWSPACE3));

    // The three sessions this fixture carries FOR this test: rows 943197,
    // 943232 and 948060, which name github.com/AI-Substrate/pij.
    for foreign in [
        "c5967bc2-f25c-438e-a23f-a61c15de973e",
        "c800c9ff-86e7-4a5f-bdc3-f63517243af6",
        "1fe494c6-e5c5-4e46-a9b4-4691b9411c3c",
    ] {
        let outcome = ours.resolve(&IngestInput::Native {
            session_id: foreign.to_owned(),
            harness: Harness::MetricsDb,
            folder: PathBuf::from("/Users/agent/substrate/flowspace/flowspace3"),
        });
        assert!(
            outcome.is_err(),
            "session {foreign} belongs to another repository and must be invisible, not merely \
             unread — this store holds every repository on the machine at once"
        );
    }

    // And the same reader scoped to the OTHER repo finds them, which is what
    // makes the exclusion above a scoping result rather than a broken query.
    let theirs = MetricsDbSource::new(scratch.database(), RepoScope::remote_url(FOREIGN));
    let files = theirs
        .resolve(&IngestInput::Native {
            session_id: "c5967bc2-f25c-438e-a23f-a61c15de973e".to_owned(),
            harness: Harness::MetricsDb,
            folder: PathBuf::from("/anywhere"),
        })
        .expect("scoped to its own repository, the session resolves");
    assert_eq!(files.len(), 1);
}

#[test]
fn the_fixtures_own_ninety_seven_of_one_hundred_claim_still_holds() {
    // An independent cross-check of the scope predicate, deliberately computed
    // the packet's original way — a LIKE over the whole record. It is a fine
    // tripwire over frozen bytes and a correctness bug as a production scope,
    // because it is a substring search over conversation prose; keeping both
    // numbers here is what proves the field-based scope agrees with it.
    let database = fixtures_root().join("metrics_db/metrics.sqlite3");
    let connection = Connection::open(&database).expect("open the committed fixture read-only");

    let by_substring: i64 = connection
        .query_row(
            "select count(*) from metrics where event_json like '%flowspace3%'",
            [],
            |row| row.get(0),
        )
        .expect("substring count");
    let by_field: i64 = connection
        .query_row(
            "select count(*) from metrics where json_extract(event_json, '$.a.\"1\"') = ?1",
            [FLOWSPACE3],
            |row| row.get(0),
        )
        .expect("field count");
    let total: i64 = connection
        .query_row("select count(*) from metrics", [], |row| row.get(0))
        .expect("total");

    assert_eq!(total, 100);
    assert_eq!(by_substring, 97);
    assert_eq!(
        by_field, 97,
        "the field-based scope and the fixture's documented substring count must agree"
    );
}

#[test]
fn the_unscoped_read_has_no_spelling() {
    // An API-SHAPE proof, which is the bar u2 sets: this test is a compile-time
    // claim wearing a runtime test's clothes. `MetricsDbSource::new` takes a
    // `RepoScope` by value and `RepoScope` has no `Default`, so there is no
    // unscoped constructor to call and no `None` to pass. If someone adds one,
    // the line below stops being the only way to build a reader and this
    // comment is the record of why that would be wrong.
    let scratch = Scratch::new("api-shape");
    let source = MetricsDbSource::new(scratch.database(), RepoScope::remote_url(FLOWSPACE3));
    assert_eq!(source.scope().as_str(), FLOWSPACE3);
    assert_eq!(source.harness(), Harness::MetricsDb);
}

// --- the copilot dialect ------------------------------------------------

#[test]
fn the_copilot_dialect_is_read_from_the_stores_own_tool_column() {
    let fixture = Fixture::new("copilot");
    let records = read_all(&fixture, COPILOT);

    // user.message ×1 + assistant.message ×3 + tool.execution_start ×1.
    assert_eq!(records.len(), 5, "26 rows, five of which are conversation");

    let human = records
        .iter()
        .filter(|record| record.role == TurnRole::Human)
        .count();
    assert_eq!(human, 1);

    // The copilot event name lives at `v."0".type`, NOT `v."0".name` as the
    // plan packet said — no row in this store has that path. Confirmed a packet
    // typo by the PM, 2026-08-28. If this reader had looked for `name`, every
    // record here would be missing and the count above would be zero.
    assert!(
        records.iter().any(|record| !record.body.is_empty()),
        "reading the event name from the wrong path yields no prose at all"
    );
}

#[test]
fn a_copilot_tool_call_and_its_result_land_on_one_turn() {
    let fixture = Fixture::new("copilot-tools");
    let paired = read_all(&fixture, COPILOT)
        .into_iter()
        .find(|record| record.items.len() >= 2)
        .expect("the tool.execution_start / tool.execution_complete pair joins on toolCallId");

    assert!(matches!(paired.items[0], TurnItem::ToolCall { .. }));
    assert!(matches!(paired.items[1], TurnItem::ToolResult { .. }));
}

#[test]
fn an_event_type_this_reader_has_never_heard_of_is_dropped_not_fatal() {
    let scratch = Scratch::new("unknown-type");
    let connection = Connection::open(scratch.database()).expect("open scratch");

    // The twentieth copilot event type, shipped by someone else, mid-session.
    connection
        .execute(
            "insert into metrics (id, event_json, attempts, next_retry_at, event_ts, event_kind, \
             tool, external_session_id) values (?1, ?2, 0, 0, 1787778300, 5, \
             'github-copilot-cli', ?3)",
            (
                999_001,
                format!(
                    r#"{{"t":1787778300,"e":5,"v":{{"0":{{"type":"session.telemetry_v2_beta","timestamp":"2026-08-26T21:05:00.000Z","data":{{"whatever":true}}}}}},"a":{{"1":"{FLOWSPACE3}"}}}}"#
                ),
                COPILOT,
            ),
        )
        .expect("insert an unknown event type");

    // And a row from a tool this reader has no dialect for at all.
    connection
        .execute(
            "insert into metrics (id, event_json, attempts, next_retry_at, event_ts, event_kind, \
             tool, external_session_id) values (?1, ?2, 0, 0, 1787778301, 5, \
             'some-agent-shipped-next-year', ?3)",
            (
                999_002,
                format!(
                    r#"{{"t":1787778301,"e":5,"v":{{"0":{{"type":"user.message","data":{{"content":"invisible"}}}}}},"a":{{"1":"{FLOWSPACE3}"}}}}"#
                ),
                COPILOT,
            ),
        )
        .expect("insert an unknown tool");

    let source = MetricsDbSource::new(scratch.database(), RepoScope::remote_url(FLOWSPACE3));
    let file = source
        .resolve(&IngestInput::Native {
            session_id: COPILOT.to_owned(),
            harness: Harness::MetricsDb,
            folder: PathBuf::from("/Users/agent/substrate/flowspace/flowspace3"),
        })
        .expect("resolve")
        .into_iter()
        .find(|file| file.kind == SessionKind::Main)
        .expect("main");

    let batch = source
        .read_incremental(&file, None)
        .expect("an unknown event type must not fail the ingest");

    assert_eq!(
        batch.records.len(),
        5,
        "the unknown rows are dropped and the surrounding records still parse"
    );
    // The cursor still advances PAST the dropped rows, or the reader stalls on
    // them forever and the conversation silently stops updating.
    assert_eq!(batch.cursor, SourceCursor::RowId { rowid: 999_002 });
}

// --- the cursor ---------------------------------------------------------

#[test]
fn a_pruned_store_is_reported_as_a_rescan_rather_than_going_quiet() {
    let fixture = Fixture::new("prune");
    let file = session_file(&fixture, MAIN);

    // A cursor above every row this session has: what a held cursor looks like
    // after the store self-pruned underneath it. Read as "nothing new" it would
    // be indistinguishable from a quiet conversation, forever.
    let stale = SourceCursor::RowId { rowid: 999_999 };
    let batch = fixture
        .source
        .read_incremental(&file, Some(&stale))
        .expect("a pruned store is readable, not an error");

    assert!(
        batch.rescanned,
        "a cursor above max(rowid) means the store pruned"
    );
    assert_eq!(
        batch.records.len(),
        16,
        "a rescan returns the whole conversation"
    );
}

#[test]
fn a_session_with_no_rows_in_scope_is_not_mistaken_for_a_prune() {
    let scratch = Scratch::new("empty-not-pruned");
    let source = MetricsDbSource::new(scratch.database(), RepoScope::remote_url(FLOWSPACE3));

    // A session file naming rows that exist only in the OTHER repository, so
    // `max(rowid)` in scope is NULL rather than small. Treating that as a prune
    // would make every empty poll a full re-read.
    let file = fs3_core::SessionFile {
        path: scratch.database(),
        session_id: "c5967bc2-f25c-438e-a23f-a61c15de973e".to_owned(),
        parent_session_id: None,
        kind: SessionKind::Main,
        harness: Harness::MetricsDb,
    };

    let held = SourceCursor::RowId { rowid: 500 };
    let batch = source
        .read_incremental(&file, Some(&held))
        .expect("an empty scoped session reads cleanly");

    assert!(batch.records.is_empty());
    assert!(!batch.rescanned, "no rows is no data, not a prune");
    assert_eq!(
        batch.cursor, held,
        "an empty poll leaves the cursor where it was"
    );
}

#[test]
fn the_subagent_names_its_parent_so_its_work_is_not_invisible() {
    let fixture = Fixture::new("subagent");
    let files = fixture
        .source
        .resolve(&fixture.input())
        .expect("resolve finds the sidecar conversation");

    let child = files
        .iter()
        .find(|file| file.kind == SessionKind::Subagent)
        .expect("the subagent conversation");

    assert_eq!(child.session_id, SUBAGENT);
    assert_eq!(child.parent_session_id.as_deref(), Some(MAIN));
    // One SessionFile per SESSION, not per file: for this store the path is the
    // database, which is what keeps the cursor per-conversation.
    assert_eq!(child.path, fixture.scratch.database());
}

// --- helpers ------------------------------------------------------------

fn session_file(fixture: &Fixture, session: &str) -> fs3_core::SessionFile {
    fs3_core::SessionFile {
        path: fixture.scratch.database(),
        session_id: session.to_owned(),
        parent_session_id: None,
        kind: SessionKind::Main,
        harness: Harness::MetricsDb,
    }
}

fn read_all(fixture: &Fixture, session: &str) -> Vec<fs3_core::RawRecord> {
    let file = session_file(fixture, session);
    fixture
        .source
        .read_incremental(&file, None)
        .expect("a full read")
        .records
}
