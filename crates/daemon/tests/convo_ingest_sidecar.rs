//! The composed ingest path, end to end, for a claude session with a sidecar.
//!
//! This exists because cross-model review (F-002, then F-008) found the same
//! promise asserted twice on evidence that did not reach it. ac-0004 says a
//! subagent sidecar is ingested as a child conversation LINKED to its parent.
//! The first implementation left that link in an in-memory `SessionFile` and an
//! `IngestReport` the worker discarded; the first test then asserted the link by
//! constructing the parent field by hand and reading the STORE list directly.
//!
//! Both of the seams the receipt claims — the composition root DERIVING the
//! parent, and the daemon SERIALISING it as a `conv:` address — were still
//! untested. This test runs the real `ingest` against a real session tree and
//! reads the relationship back through `conversations::list`, the surface the
//! CLI actually calls, so removing either seam turns it red.

mod support;

use std::path::Path;

use fs3_core::{Config, DatabaseConfig, Harness};
use fs3_daemon::conversations::{ListRequest, list};
use fs3_daemon::convo_ingest::{IngestRequest, conversation_guid, ingest, run};
use fs3_daemon::wiring::AppState;

/// One session id per test. They share a process — `HOME` is global and the
/// ingest lock is keyed on the conversation — so two tests on ONE session id
/// contend with each other and one of them fails retryably. That is the reader
/// working; it was this file racing. (Found in CI, which runs the three in
/// parallel; locally they happened to interleave harmlessly.)
const SESSION_LINK: &str = "a5a5588f-0979-439f-a1bf-ddf185a089c7";
const SESSION_RERUN: &str = "b6b6699a-1a8a-4a3b-8b4c-ee2f96b19aa8";
const SESSION_CONTENDED: &str = "c7c77aab-2b9b-4b4c-9c5d-ff3fa7c2abb9";
const SIDECAR: &str = "agent-a01869bcb5e09448b";

/// The ONE home every test in this file shares.
///
/// `std::env::set_var` is process-wide, so each test setting its own would race
/// the others. One directory, written once, holding every session tree.
fn shared_home() -> std::path::PathBuf {
    static ONCE: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    ONCE.get_or_init(|| {
        let home = support::temp_dir("convo-ingest-home");
        for session in [SESSION_LINK, SESSION_RERUN, SESSION_CONTENDED] {
            session_tree(&home, session).expect("a scratch session tree");
        }
        // SAFETY: set once, to one value, before any test reads it.
        unsafe { std::env::set_var("HOME", &home) };
        home
    })
    .clone()
}

async fn stack(label: &str) -> (support::FreshDatabase, AppState) {
    let database = support::FreshDatabase::create(label).await;
    let config = Config {
        database: DatabaseConfig {
            url: database.url(),
        },
        ..Config::default()
    };
    let state = AppState::from_config(config).expect("the fake stack wires");
    fs3_store::migrate(&state.db).await.expect("migrates");
    (database, state)
}

/// One claude record, enough to be a turn the reader will emit.
fn record(session: &str, uuid: &str, kind: &str, text: &str) -> String {
    format!(
        r#"{{"type":"{kind}","uuid":"{uuid}","parentUuid":null,"sessionId":"{session}","cwd":"{cwd}","timestamp":"2026-08-27T09:00:00Z","message":{{"role":"{role}","content":[{{"type":"text","text":"{text}"}}]}}}}"#,
        cwd = "/srv/work/repo",
        role = if kind == "user" { "user" } else { "assistant" },
    )
}

/// A claude project directory holding one session and one subagent sidecar.
///
/// Built rather than copied from the committed fixtures: those are byte-pinned
/// and asserted unchanged, and this needs a `cwd` that matches the folder the
/// ingest is asked for.
fn session_tree(home: &Path, session: &str) -> std::io::Result<()> {
    let slug = "-srv-work-repo";
    let projects = home.join(".claude/projects").join(slug);
    std::fs::create_dir_all(projects.join(session).join("subagents"))?;

    std::fs::write(
        projects.join(format!("{session}.jsonl")),
        format!(
            "{}\n{}\n",
            record(
                session,
                "11111111-1111-4111-8111-111111111111",
                "user",
                "parent ask"
            ),
            record(
                session,
                "22222222-2222-4222-8222-222222222222",
                "assistant",
                "parent answer"
            ),
        ),
    )?;
    std::fs::write(
        projects
            .join(session)
            .join("subagents")
            .join(format!("{SIDECAR}.jsonl")),
        format!(
            "{}\n",
            record(
                session,
                "33333333-3333-4333-8333-333333333333",
                "user",
                "child ask"
            ),
        ),
    )?;
    Ok(())
}

#[tokio::test]
async fn a_sidecar_is_ingested_as_a_child_that_names_its_parent() {
    let (database, state) = stack("convo-ingest-sidecar").await;
    let _home = shared_home();

    let report = ingest(
        &state,
        &IngestRequest {
            pij_id: None,
            session_id: Some(SESSION_LINK.to_string()),
            harness: Some("claude".to_string()),
            folder: Some("/srv/work/repo".to_string()),
        },
    )
    .await
    .expect("the composed ingest runs");

    assert_eq!(
        report.sessions.len(),
        2,
        "the main file and its sidecar are both ingested: {report:?}"
    );

    // The surface a CLI caller reads, not the store API the first test used.
    let listed = list(&state, &ListRequest::default())
        .await
        .expect("the daemon lists what ingest stored");
    assert_eq!(listed.conversations.len(), 2, "parent and child");

    let child = listed
        .conversations
        .iter()
        .find(|row| row.title.as_deref() == Some(&format!("subagent {SIDECAR}")))
        .expect("the sidecar is its own conversation");
    let parent = listed
        .conversations
        .iter()
        .find(|row| row.title.as_deref() == Some(&format!("session {SESSION_LINK}")))
        .expect("the main session is its own conversation");

    assert_eq!(
        child.parent.as_deref(),
        Some(parent.address.as_str()),
        "the child names its parent as a `conv:` address the caller can pass \
         straight to `get` — the composition root derived it and the daemon \
         serialised it, which is what this test exists to hold"
    );
    assert_eq!(parent.parent, None, "a main session has no parent");

    database.destroy(state.db.clone()).await;
}

/// Re-running a job that already stored some of its files stores nothing twice.
///
/// Prime's instruction after round 5: the partial-progress claim does not stay a
/// belief. A retryable failure re-runs the WHOLE job, so the session files that
/// already succeeded are read again — and the only thing standing between that
/// and a duplicated conversation is the ordinal ledger. This proves it rather
/// than asserting it.
#[tokio::test]
async fn re_running_an_ingest_stores_nothing_a_second_time() {
    let (database, state) = stack("convo-ingest-rerun").await;
    let _home = shared_home();

    let request = IngestRequest {
        pij_id: None,
        session_id: Some(SESSION_RERUN.to_string()),
        harness: Some("claude".to_string()),
        folder: Some("/srv/work/repo".to_string()),
    };

    let first = ingest(&state, &request)
        .await
        .expect("the first ingest runs");
    assert!(first.turns_new > 0, "the first run stores turns");

    let second = ingest(&state, &request)
        .await
        .expect("re-running the same job is not an error");
    assert_eq!(
        second.turns_new, 0,
        "a re-run stores NOTHING: the ledger recognises every ordinal it already has"
    );
    // MEASURED, and worth stating because the first version of this test
    // asserted the wrong mechanism: `deduped` is ZERO here, not equal to the
    // first run's turns. The cursor is what makes a re-run cheap — the reader
    // resumes at the offset it stopped at and produces NO records, so there is
    // nothing for the ledger to recognise. The ledger is the SECOND line of
    // defence and it covers the RESCAN case, where a rotation makes a reader
    // restart from zero; that is proven in the store suite, against Postgres.
    assert_eq!(
        second.records_read, 0,
        "the cursor resumed at EOF, so nothing was re-read in the first place"
    );
    assert_eq!(second.deduped, 0, "and therefore nothing was deduped");

    let listed = list(&state, &ListRequest::default())
        .await
        .expect("listing after the re-run");
    let total: i64 = listed.conversations.iter().map(|row| row.turns).sum();
    assert_eq!(
        total, first.turns_new as i64,
        "the conversation holds exactly what the first run put in it"
    );

    database.destroy(state.db.clone()).await;
}

/// A poll of a conversation another poll is holding does NOT settle successful.
///
/// Round 4 found the first attempt at this settling `done` while reading
/// nothing, which loses the delta the job was fired for. The contended run now
/// fails RETRYABLY, so the runner's existing backoff re-runs it.
#[tokio::test]
async fn a_contended_conversation_fails_retryably_rather_than_settling_done() {
    let (database, state) = stack("convo-ingest-contended").await;
    let _home = shared_home();

    // Hold the lock the way a concurrent poll would, on its own connection.
    let guid = conversation_guid(Harness::Claude, SESSION_CONTENDED);
    let holder =
        fs3_store::ingest_cursors::try_with_conversation_lock(&state.db, &guid, || async {
            let payload = serde_json::json!({
                "session_id": SESSION_CONTENDED,
                "harness": "claude",
                "folder": "/srv/work/repo",
            });
            run(&state, payload).await
        })
        .await
        .expect("the outer lock is taken")
        .expect("the outer closure ran");

    let failure = holder.expect_err("a contended run must not settle successful");
    assert!(
        failure.retryable,
        "and it must be RETRYABLE, so the runner re-runs it: {failure:?}"
    );

    database.destroy(state.db.clone()).await;
}
