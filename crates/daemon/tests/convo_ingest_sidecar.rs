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

const SESSION: &str = "a5a5588f-0979-439f-a1bf-ddf185a089c7";
const SIDECAR: &str = "agent-a01869bcb5e09448b";

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
fn record(uuid: &str, kind: &str, text: &str) -> String {
    format!(
        r#"{{"type":"{kind}","uuid":"{uuid}","parentUuid":null,"sessionId":"{SESSION}","cwd":"{cwd}","timestamp":"2026-08-27T09:00:00Z","message":{{"role":"{role}","content":[{{"type":"text","text":"{text}"}}]}}}}"#,
        cwd = "/srv/work/repo",
        role = if kind == "user" { "user" } else { "assistant" },
    )
}

/// A claude project directory holding one session and one subagent sidecar.
///
/// Built rather than copied from the committed fixtures: those are byte-pinned
/// and asserted unchanged, and this needs a `cwd` that matches the folder the
/// ingest is asked for.
fn session_tree(home: &Path) -> std::io::Result<()> {
    let slug = "-srv-work-repo";
    let projects = home.join(".claude/projects").join(slug);
    std::fs::create_dir_all(projects.join(SESSION).join("subagents"))?;

    std::fs::write(
        projects.join(format!("{SESSION}.jsonl")),
        format!(
            "{}\n{}\n",
            record("11111111-1111-4111-8111-111111111111", "user", "parent ask"),
            record(
                "22222222-2222-4222-8222-222222222222",
                "assistant",
                "parent answer"
            ),
        ),
    )?;
    std::fs::write(
        projects
            .join(SESSION)
            .join("subagents")
            .join(format!("{SIDECAR}.jsonl")),
        format!(
            "{}\n",
            record("33333333-3333-4333-8333-333333333333", "user", "child ask"),
        ),
    )?;
    Ok(())
}

#[tokio::test]
async fn a_sidecar_is_ingested_as_a_child_that_names_its_parent() {
    let (database, state) = stack("convo-ingest-sidecar").await;
    let home = support::temp_dir("convo-ingest-home");
    session_tree(&home).expect("a scratch session tree");
    // SAFETY: single-threaded test; the reader resolves its store beneath HOME.
    unsafe { std::env::set_var("HOME", &home) };

    let report = ingest(
        &state,
        &IngestRequest {
            pij_id: None,
            session_id: Some(SESSION.to_string()),
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
        .find(|row| row.title.as_deref() == Some(&format!("session {SESSION}")))
        .expect("the main session is its own conversation");

    assert_eq!(
        child.parent.as_deref(),
        Some(parent.address.as_str()),
        "the child names its parent as a `conv:` address the caller can pass \
         straight to `get` — the composition root derived it and the daemon \
         serialised it, which is what this test exists to hold"
    );
    assert_eq!(parent.parent, None, "a main session has no parent");

    std::fs::remove_dir_all(&home).ok();
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
    let home = support::temp_dir("convo-ingest-rerun-home");
    session_tree(&home).expect("a scratch session tree");
    // SAFETY: single-threaded test; the reader resolves its store beneath HOME.
    unsafe { std::env::set_var("HOME", &home) };

    let request = IngestRequest {
        pij_id: None,
        session_id: Some(SESSION.to_string()),
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

    std::fs::remove_dir_all(&home).ok();
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
    let home = support::temp_dir("convo-ingest-contended-home");
    session_tree(&home).expect("a scratch session tree");
    // SAFETY: single-threaded test; the reader resolves its store beneath HOME.
    unsafe { std::env::set_var("HOME", &home) };

    // Hold the lock the way a concurrent poll would, on its own connection.
    let guid = conversation_guid(Harness::Claude, SESSION);
    let holder =
        fs3_store::ingest_cursors::try_with_conversation_lock(&state.db, &guid, || async {
            let payload = serde_json::json!({
                "session_id": SESSION,
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

    std::fs::remove_dir_all(&home).ok();
    database.destroy(state.db.clone()).await;
}
