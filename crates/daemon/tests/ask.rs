//! `POST /ask`, end to end against the real router.
//!
//! The chat model is a SCRIPTED fake, so these tests are about the things the
//! daemon is actually responsible for — the envelope shape, the trace, the
//! citation record, and what a bounded run reports — rather than about whether
//! a hosted model is any good at answering. Model quality is the evaluation
//! suite's job (`w-ask-eval`), and it is a different question from this one.
//!
//! Every test mints its own throwaway database and drops it, per the fleet
//! safety rule: a shared test database plus ambient provider config is how two
//! seats once bought 150 summaries and 2,475 vectors in fifteen minutes. The
//! providers here are fakes, so nothing reaches a network.

mod support;

use std::sync::Arc;

use fs3_core::envelope::Envelope;
use fs3_core::{ChatTurn, Config, DatabaseConfig, ToolCall};
use fs3_daemon::router;
use fs3_daemon::wiring::AppState;
use fs3_testkit::fakes::FakeChatProvider;
use serde_json::{Value, json};

/// Wire a daemon whose chat model says exactly what the test tells it to.
async fn daemon_answering_with(
    label: &str,
    turns: Vec<ChatTurn>,
) -> (String, String, support::FreshDatabase, fs3_store::PgPool) {
    let database = support::FreshDatabase::create(label).await;
    let config = Config {
        database: DatabaseConfig {
            url: database.url(),
        },
        ..Config::default()
    };

    let mut state = AppState::from_config(config).expect("the fake arms wire with no keys");
    fs3_store::migrate(&state.db).await.expect("migrates");

    // The port is public precisely so a test can substitute a scripted model
    // without inventing a second composition root.
    state.agent = Arc::new(FakeChatProvider::scripted(turns));

    // Kept out of the router so the test can close the pool when it destroys
    // the database — `Drop` cannot await, so cleanup is explicit here.
    let pool = state.db.clone();
    // Since #43 every route sits behind the daemon key, so the test carries one
    // like any real caller.
    let auth = support::auth(label);
    let base = support::spawn(router(state, auth.auth)).await;
    (base, auth.key, database, pool)
}

async fn post_ask(base: &str, key: &str, question: &str) -> Envelope<Value> {
    reqwest::Client::new()
        .post(format!("{base}/ask"))
        .bearer_auth(key)
        .json(&json!({ "question": question }))
        .send()
        .await
        .expect("the daemon answers")
        .json()
        .await
        .expect("the answer is an envelope")
}

fn prose(text: &str) -> ChatTurn {
    ChatTurn {
        content: Some(text.to_string()),
        tool_calls: vec![],
        tokens_used: Some(10),
    }
}

fn tool_call(name: &str, arguments: &str) -> ChatTurn {
    ChatTurn {
        content: None,
        tool_calls: vec![ToolCall {
            id: "call-1".into(),
            name: name.into(),
            arguments: arguments.into(),
        }],
        tokens_used: Some(10),
    }
}

#[tokio::test]
async fn a_question_comes_back_as_an_ask_envelope_naming_its_scope() {
    let (base, key, database, pool) = daemon_answering_with(
        "ask-answer",
        vec![prose("the watcher debounces per directory")],
    )
    .await;

    let envelope = post_ask(&base, &key, "how does the watcher decide what to rescan?").await;

    assert!(envelope.ok, "{envelope:?}");
    assert_eq!(envelope.command, "ask");
    let data = envelope.data.expect("an ask report");
    assert_eq!(
        data["answer"], "the watcher debounces per directory",
        "the answer is carried verbatim"
    );
    assert_eq!(data["stopped"], "answered");
    assert_eq!(
        data["question"],
        "how does the watcher decide what to rescan?"
    );
    // Scope rides on the envelope for the same reason it does on search: an
    // answer drawn from one repository when the caller expected all of them is
    // indistinguishable from a wrong answer unless the scope is visible.
    assert!(
        envelope.meta.expect("meta")["scope"].is_object(),
        "the scope must be on the envelope"
    );

    database.destroy(pool).await;
}

#[tokio::test]
async fn a_search_against_an_empty_index_is_reported_to_the_model_not_hidden() {
    let (base, key, database, pool) = daemon_answering_with(
        "ask-trace",
        vec![
            tool_call("search", r#"{"query":"watcher debounce"}"#),
            prose("I could not find evidence for that in the index"),
        ],
    )
    .await;

    let envelope = post_ask(&base, &key, "how does the watcher work?").await;
    let data = envelope.data.expect("an ask report");

    let trace = data["trace"].as_array().expect("a trace");
    assert_eq!(trace.len(), 1, "the tool call was run and recorded");
    assert_eq!(trace[0]["tool"], "search");

    // An index with nothing in it is NOT "found nothing" — the store answers
    // FS3-E-QUERY-NO-INDEX, "no embeddings exist at all". That distinction is
    // worth preserving all the way to the model: "nothing matched your query"
    // invites it to conclude the subject does not exist, while "there is no
    // index yet" is a fact about the SYSTEM. Handing the real error across
    // keeps the model from turning an unindexed machine into a confident
    // denial, and the run still completes because tool errors are data.
    assert_eq!(
        trace[0]["failed"], true,
        "an empty index is an error the model is told about, not one it is shielded from"
    );
    assert_eq!(data["stopped"], "answered");
    assert!(
        data["citations"].as_array().expect("citations").is_empty(),
        "nothing was read, so nothing is cited"
    );

    database.destroy(pool).await;
}

#[tokio::test]
async fn an_unknown_tool_is_reported_to_the_model_rather_than_failing_the_request() {
    let (base, key, database, pool) = daemon_answering_with(
        "ask-badtool",
        vec![
            tool_call("summon_oracle", "{}"),
            prose("recovered without it"),
        ],
    )
    .await;

    let envelope = post_ask(&base, &key, "anything").await;

    // The HTTP call SUCCEEDS: a model asking for a tool that does not exist is
    // a recoverable turn, not a failed request. This is the property that let
    // the prototype recover unaided from an ambiguous address.
    assert!(
        envelope.ok,
        "an invented tool name must not fail the request"
    );
    let data = envelope.data.expect("an ask report");
    assert_eq!(data["answer"], "recovered without it");
    assert_eq!(
        data["trace"][0]["failed"], true,
        "the bad call is recorded as failed so the trace stays honest"
    );

    database.destroy(pool).await;
}

#[tokio::test]
async fn a_model_that_never_stops_calling_tools_is_cut_off_without_inventing_an_answer() {
    // More tool calls than the configured iteration bound allows.
    let turns = (0..12)
        .map(|_| tool_call("search", r#"{"query":"again"}"#))
        .collect();
    let (base, key, database, pool) = daemon_answering_with("ask-bound", turns).await;

    let envelope = post_ask(&base, &key, "a question it will never answer").await;
    let data = envelope.data.expect("an ask report");

    assert_eq!(data["stopped"], "max_iterations");
    // The bound is the whole point: no answer is better than a fabricated one,
    // and the caller is told which bound stopped it.
    assert!(
        data["answer"].is_null(),
        "a bounded run must not present something as the answer"
    );
    assert_eq!(
        data["iterations"], 8,
        "the configured default bound applied"
    );

    database.destroy(pool).await;
}
