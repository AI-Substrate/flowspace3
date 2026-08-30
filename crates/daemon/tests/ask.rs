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
use std::sync::atomic::{AtomicUsize, Ordering};

use fs3_core::envelope::Envelope;
use fs3_core::{
    BlobRef, ChatMessage, ChatTurn, Config, DatabaseConfig, Element, ElementKind, RepoIdentity,
    Span, ToolCall, content_hash, element_address,
};
use fs3_daemon::router;
use fs3_daemon::wiring::AppState;
use fs3_store::{NewEmbedding, SourceKind};
use fs3_testkit::fakes::FakeChatProvider;
use serde_json::{Value, json};

struct FailingChatProvider {
    turns: Vec<ChatTurn>,
    calls: AtomicUsize,
}

#[async_trait::async_trait]
impl fs3_core::ChatProvider for FailingChatProvider {
    async fn turn(
        &self,
        _messages: &[ChatMessage],
        _tools: &[fs3_core::ToolSchema],
    ) -> fs3_core::Result<ChatTurn> {
        let call = self.calls.fetch_add(1, Ordering::Relaxed);
        self.turns.get(call).cloned().ok_or_else(|| {
            fs3_core::Error::Provider("injected chat failure after evidence".to_string())
        })
    }

    fn key(&self) -> String {
        "failing-chat@1".to_string()
    }

    fn max_input_tokens(&self) -> usize {
        100_000
    }
}
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

/// Seed one searchable element and return the exact address search emits.
async fn seed_search_hit(state: &AppState, question: &str) -> String {
    let root = "/srv/ask-trace";
    let identity = RepoIdentity::from_path(std::path::Path::new(root));
    let identity_text = identity.to_string();
    let worktree = fs3_store::register_worktree(&state.db, &identity, root, Some("main"))
        .await
        .expect("registers the fixture worktree");
    let path = "src/watcher.rs";
    let text = "watcher debounce returns only changed directories";
    let child_address = format!("{path}::debounce");
    let child = Element::new(
        ElementKind::Function,
        "function_item",
        "debounce",
        &child_address,
        Span::new(1, 1),
        text,
    );
    let file = Element::new(
        ElementKind::File,
        "source_file",
        path,
        path,
        Span::new(1, 1),
        "fixture file containing the watcher function",
    )
    .with_children(vec![child]);
    let blob = BlobRef::new("1111111111111111111111111111111111111111").expect("a blob key");

    fs3_store::upsert_element_tree(&state.db, &blob, "test-parser@1", &file, |_| false)
        .await
        .expect("stores the fixture element");
    fs3_store::sync_worktree_files(&state.db, worktree, &[(path.to_string(), blob)])
        .await
        .expect("maps the fixture file");

    let vector = state
        .embedder_for(&identity_text)
        .embed(&[question.to_string()])
        .await
        .expect("the fake embeds")
        .pop()
        .expect("one vector");
    let raw_hash = content_hash(text.as_bytes());
    fs3_store::put_embeddings(
        &state.db,
        &state.embedder_key(&identity_text),
        &[NewEmbedding {
            source_hash: &raw_hash,
            source_kind: SourceKind::Raw,
            vector: &vector,
            truncated: false,
        }],
    )
    .await
    .expect("stores the fixture vector");

    element_address(Some(&identity_text), &child_address)
}

async fn daemon_with_search_hit_using<F, C>(
    label: &str,
    question: &str,
    configure: C,
    agent: F,
) -> (
    String,
    String,
    String,
    support::FreshDatabase,
    fs3_store::PgPool,
)
where
    F: FnOnce(&str) -> Arc<dyn fs3_core::ChatProvider>,
    C: FnOnce(&mut Config),
{
    let database = support::FreshDatabase::create(label).await;
    let mut config = Config {
        database: DatabaseConfig {
            url: database.url(),
        },
        ..Config::default()
    };
    configure(&mut config);
    let mut state = AppState::from_config(config).expect("the fake stack wires");
    fs3_store::migrate(&state.db).await.expect("migrates");
    let address = seed_search_hit(&state, question).await;
    state.agent = agent(&address);

    let pool = state.db.clone();
    let auth = support::auth(label);
    let base = support::spawn(router(state, auth.auth)).await;
    (base, auth.key, address, database, pool)
}

/// Wire a daemon with one real search hit and a scripted search → get → answer run.
async fn daemon_with_search_hit(
    label: &str,
    question: &str,
) -> (
    String,
    String,
    String,
    support::FreshDatabase,
    fs3_store::PgPool,
) {
    daemon_with_search_hit_using(
        label,
        question,
        |_| {},
        |address| {
            Arc::new(FakeChatProvider::scripted(vec![
                tool_call("search", &json!({"query": question}).to_string()),
                tool_call("get", &json!({"address": address}).to_string()),
                prose("the watcher returns only changed directories"),
            ]))
        },
    )
    .await
}

/// Wire a daemon whose active model has an index but whose search resolves no hits.
async fn daemon_with_no_hit_search(
    label: &str,
    question: &str,
) -> (String, String, support::FreshDatabase, fs3_store::PgPool) {
    let database = support::FreshDatabase::create(label).await;
    let config = Config {
        database: DatabaseConfig {
            url: database.url(),
        },
        ..Config::default()
    };
    let mut state = AppState::from_config(config).expect("the fake stack wires");
    fs3_store::migrate(&state.db).await.expect("migrates");
    let vector = state
        .embedder_for("")
        .embed(&[question.to_string()])
        .await
        .expect("the fake embeds")
        .pop()
        .expect("one vector");
    let orphan_hash = "a".repeat(64);
    fs3_store::put_embeddings(
        &state.db,
        &state.embedder_key(""),
        &[NewEmbedding {
            source_hash: &orphan_hash,
            source_kind: SourceKind::Raw,
            vector: &vector,
            truncated: false,
        }],
    )
    .await
    .expect("stores an index row with no resolvable element");
    state.agent = Arc::new(FakeChatProvider::scripted(vec![
        tool_call("search", &json!({"query": question}).to_string()),
        prose("I could not find evidence for that in the index"),
        prose("I could not find evidence for that in the index"),
    ]));

    let pool = state.db.clone();
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
async fn a_daemon_that_cannot_answer_says_so_on_the_envelope_not_in_the_prose() {
    // The reported bug, as a regression. A daemon wired to the offline fake
    // returned ok:true with answer "The offline fake has no scripted answer."
    // and citations [] — so a machine consumer, which our own envelope rule
    // tells to branch on `ok` alone, banked a placeholder as a finding.
    // `grounded:false` and a suspicious next_action were both present and both
    // in the wrong place: neither is where a machine looks.
    //
    // An UNSCRIPTED fake is the production shape here, so this test asks for
    // exactly that rather than scripting one.
    let database = support::FreshDatabase::create("ask-cannot-answer").await;
    let config = Config {
        database: DatabaseConfig {
            url: database.url(),
        },
        ..Config::default()
    };
    let state = AppState::from_config(config).expect("the fake arms wire with no keys");
    fs3_store::migrate(&state.db).await.expect("migrates");
    let pool = state.db.clone();
    let auth = support::auth("ask-cannot-answer");
    let key = auth.key.clone();
    let base = support::spawn(router(state, auth.auth)).await;

    let envelope = post_ask(&base, &key, "anything at all").await;

    assert!(
        !envelope.ok,
        "a daemon that cannot answer must FAIL, not succeed with a placeholder: {envelope:?}"
    );
    let failure = envelope.error.expect("a failure");
    assert_eq!(failure.code, "FS3-E-PROVIDER-CANNOT-ANSWER");
    // The fix has to name the thing to change, or the caller is stuck knowing
    // only that it did not work.
    assert!(failure.fix.contains("[agent] active"), "{}", failure.fix);
    // And nothing was spent finding out.
    assert!(envelope.data.is_none(), "no report is produced");

    database.destroy(pool).await;
}

#[tokio::test]
async fn a_question_comes_back_as_an_ask_envelope_naming_its_scope() {
    // Twice: an answer with no tool calls is ungrounded, so the loop pushes
    // back once and demands evidence before it will publish anything.
    let (base, key, database, pool) = daemon_answering_with(
        "ask-answer",
        vec![
            prose("the watcher debounces per directory"),
            prose("the watcher debounces per directory"),
        ],
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
async fn search_hits_and_full_reads_are_distinct_in_the_trace() {
    let question = "how does the watcher debounce changed directories?";
    let (base, key, address, database, pool) =
        daemon_with_search_hit("ask-search-hits", question).await;

    let envelope = post_ask(&base, &key, question).await;
    assert!(envelope.ok, "{envelope:?}");
    let data = envelope.data.expect("an ask report");
    let trace = data["trace"].as_array().expect("a trace");

    assert_eq!(trace.len(), 2);
    assert_eq!(trace[0]["tool"], "search");
    assert_eq!(trace[0]["search_hits"], json!([address]));
    assert_eq!(trace[1]["tool"], "get");
    assert_eq!(trace[1]["search_hits"], json!([]));
    assert_eq!(data["citations"], json!([address]));
    assert_eq!(data["coverage"]["iterations_used"], data["iterations"]);
    assert_eq!(data["coverage"]["iteration_limit"], 8);
    assert_eq!(data["coverage"]["retrieval_top_k"], json!([6]));
    assert_eq!(data["coverage"]["exhaustive"], false);

    database.destroy(pool).await;
}

#[tokio::test]
async fn an_unmatched_path_filter_is_reported_to_the_model_as_a_bad_filter() {
    let question = "where is watcher debounce implemented?";
    let database = support::FreshDatabase::create("ask-path-unmatched").await;
    let config = Config {
        database: DatabaseConfig {
            url: database.url(),
        },
        ..Config::default()
    };
    let mut state = AppState::from_config(config).expect("the fake stack wires");
    fs3_store::migrate(&state.db).await.expect("migrates");
    seed_search_hit(&state, question).await;
    let chat = Arc::new(FakeChatProvider::scripted(vec![
        tool_call("search", r#"{"query":"watcher debounce","path":"apps/**"}"#),
        prose("the path filter matched no indexed paths, so this does not prove absence"),
        prose("the path filter matched no indexed paths, so this does not prove absence"),
    ]));
    state.agent = chat.clone();

    let pool = state.db.clone();
    let auth = support::auth("ask-path-unmatched");
    let base = support::spawn(router(state, auth.auth)).await;
    let envelope = post_ask(&base, &auth.key, question).await;
    assert!(envelope.ok, "{envelope:?}");
    let data = envelope.data.expect("an ask report");
    assert_eq!(data["trace"][0]["failed"], false);
    assert_eq!(data["trace"][0]["evidence"], false);
    assert_eq!(data["coverage"]["retrieval_top_k"], json!([6]));

    let tool_result = chat
        .received_messages()
        .into_iter()
        .flatten()
        .find_map(|message| match message {
            ChatMessage::ToolResult { content, .. }
                if content.contains("PATH FILTER UNMATCHED") =>
            {
                Some(content)
            }
            _ => None,
        })
        .expect("the model receives the path diagnostic");
    assert!(tool_result.contains("apps/**"), "{tool_result}");
    assert!(tool_result.contains("src"), "{tool_result}");
    assert!(tool_result.contains("Do NOT conclude"), "{tool_result}");

    database.destroy(pool).await;
}

#[tokio::test]
async fn a_no_hit_search_records_no_addresses_and_is_not_evidence() {
    let question = "a subject absent from every mapped element";
    let (base, key, database, pool) =
        daemon_with_no_hit_search("ask-no-search-hits", question).await;

    let envelope = post_ask(&base, &key, question).await;
    assert!(envelope.ok, "{envelope:?}");
    let data = envelope.data.expect("an ask report");
    let search = &data["trace"][0];

    assert_eq!(search["tool"], "search");
    assert_eq!(search["failed"], false);
    assert_eq!(search["evidence"], false);
    assert_eq!(search["search_hits"], json!([]));
    assert_eq!(data["citations"], json!([]));

    database.destroy(pool).await;
}

#[tokio::test]
async fn an_unknown_tool_is_reported_to_the_model_rather_than_failing_the_request() {
    let (base, key, database, pool) = daemon_answering_with(
        "ask-badtool",
        vec![
            tool_call("summon_oracle", "{}"),
            // The first answer follows a FAILED call, so it is ungrounded and
            // earns the pushback; the second is what gets published.
            prose("recovered without it"),
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
    assert_eq!(
        data["grounded"], false,
        "the only tool call failed, so the answer rests on nothing"
    );

    database.destroy(pool).await;
}

#[tokio::test]
async fn iteration_exhaustion_is_a_failure_with_partial_evidence() {
    let question = "where is watcher debounce implemented?";
    let (base, key, address, database, pool) = daemon_with_search_hit_using(
        "ask-iteration-bound",
        question,
        |config| config.agent.max_iterations = 2,
        |address| {
            Arc::new(FakeChatProvider::scripted(vec![
                tool_call("search", &json!({"query": question}).to_string()),
                tool_call("get", &json!({"address": address}).to_string()),
            ]))
        },
    )
    .await;

    let envelope = post_ask(&base, &key, question).await;
    assert!(
        !envelope.ok,
        "a bound is not a successful answer: {envelope:?}"
    );
    assert!(
        envelope.data.is_none(),
        "failure envelopes never carry success data"
    );
    let failure = envelope.error.expect("an honest terminal failure");
    assert_eq!(failure.code, "FS3-E-QUERY-ASK-ITERATION-LIMIT");
    assert_eq!(failure.details["stopped"], "max_iterations");
    assert_eq!(failure.details["grounded"], false);
    assert_eq!(
        failure.details["evidence"]["label"],
        "partial evidence — no answer was synthesized"
    );
    assert_eq!(failure.details["evidence"]["citations"], json!([address]));
    assert_eq!(
        failure.details["evidence"]["findings"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    database.destroy(pool).await;
}

#[tokio::test]
async fn token_budget_exhaustion_is_a_failure_and_salvages_reads() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../eval/ask/terminal-fixtures/token-budget.json"
    ))
    .expect("the eval fixture is valid JSON");
    let expected = &fixture["expected"];
    let question = "where is watcher debounce implemented?";
    let (base, key, address, database, pool) = daemon_with_search_hit_using(
        "ask-token-bound",
        question,
        |config| {
            config.agent.token_budget = fixture["setup"]["token_budget"]
                .as_u64()
                .expect("fixture token budget")
        },
        |address| {
            Arc::new(FakeChatProvider::scripted(vec![
                tool_call("search", &json!({"query": question}).to_string()),
                tool_call("get", &json!({"address": address}).to_string()),
                tool_call("search", &json!({"query": "one more"}).to_string()),
            ]))
        },
    )
    .await;

    let envelope = post_ask(&base, &key, question).await;
    assert_eq!(envelope.ok, expected["ok"].as_bool().unwrap());
    assert!(
        envelope.data.is_none(),
        "fixture requires absent success data"
    );
    assert!(
        envelope
            .next_action
            .as_deref()
            .unwrap_or_default()
            .contains("narrower question")
            && envelope
                .next_action
                .as_deref()
                .unwrap_or_default()
                .contains("token_budget")
    );
    let failure = envelope.error.expect("an honest terminal failure");
    assert_eq!(failure.code, expected["error_code"].as_str().unwrap());
    assert_eq!(failure.details["stopped"], expected["stopped"]);
    assert_eq!(failure.details["grounded"], expected["grounded"]);
    assert_eq!(
        failure.details["evidence"]["label"],
        expected["evidence_label"]
    );
    assert!(
        failure.details["evidence"]["citations"]
            .as_array()
            .unwrap()
            .len()
            >= expected["minimum_citations"].as_u64().unwrap() as usize
    );
    assert_eq!(failure.details["evidence"]["citations"], json!([address]));
    assert_eq!(
        failure.details["evidence"]["findings"]
            .as_array()
            .unwrap()
            .len(),
        expected["findings"].as_u64().unwrap() as usize
    );

    database.destroy(pool).await;
}

#[tokio::test]
async fn provider_failure_mid_loop_is_a_failure_and_salvages_reads() {
    let question = "where is watcher debounce implemented?";
    let (base, key, address, database, pool) = daemon_with_search_hit_using(
        "ask-provider-terminal",
        question,
        |_| {},
        |address| {
            Arc::new(FailingChatProvider {
                turns: vec![
                    tool_call("search", &json!({"query": question}).to_string()),
                    tool_call("get", &json!({"address": address}).to_string()),
                ],
                calls: AtomicUsize::new(0),
            })
        },
    )
    .await;

    let envelope = post_ask(&base, &key, question).await;
    assert!(!envelope.ok);
    assert!(envelope.data.is_none());
    let failure = envelope.error.expect("an honest terminal failure");
    assert_eq!(failure.code, "FS3-E-PROVIDER-FAILED");
    assert_eq!(failure.details["stopped"], "provider_failure");
    assert_eq!(failure.details["grounded"], false);
    assert_eq!(failure.details["evidence"]["citations"], json!([address]));
    assert_eq!(
        failure.details["evidence"]["findings"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    database.destroy(pool).await;
}

#[tokio::test]
async fn answered_with_null_or_empty_text_is_impossible() {
    for (label, content) in [
        ("ask-null-answer", None),
        ("ask-empty-answer", Some(String::new())),
    ] {
        let turns = vec![
            ChatTurn {
                content: content.clone(),
                tool_calls: vec![],
                tokens_used: Some(10),
            },
            ChatTurn {
                content,
                tool_calls: vec![],
                tokens_used: Some(10),
            },
        ];
        let (base, key, database, pool) = daemon_answering_with(label, turns).await;
        let envelope = post_ask(&base, &key, "return no answer text").await;

        assert!(
            !envelope.ok,
            "empty answer text must never be successful: {envelope:?}"
        );
        let failure = envelope.error.expect("a terminal failure");
        assert_eq!(failure.code, "FS3-E-PROVIDER-FAILED");
        assert_eq!(failure.details["stopped"], "provider_failure");

        database.destroy(pool).await;
    }
}
