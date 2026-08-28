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

use fs3_core::{Config, DatabaseConfig};
use fs3_daemon::conversations::{ListRequest, list};
use fs3_daemon::convo_ingest::{IngestRequest, ingest};
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
