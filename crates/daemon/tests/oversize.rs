//! Oversized inputs must embed, not fail forever.
//!
//! The defect, measured on a live index on 2026-08-27: 59 of ~4,000 elements
//! of a real repository exceeded the embedding model's per-input token cap.
//! Azure answered each with
//! `400 Invalid 'input[0]': maximum input length is 8192 tokens`, the runner
//! retried three times into the same answer, and the jobs failed for good.
//! The content was permanently unsearchable and the queue's own memory said
//! the work was finished business.
//!
//! fs3 had a token budget for the SUM of a request and nothing at all for a
//! single member of it — and the batch planner explicitly let an item bigger
//! than the whole budget ride ALONE rather than dropping it, which is what put
//! these elements in front of the provider unaccompanied and unshortened.
//!
//! Every test here asserts on what ARRIVED at the provider, or on what the
//! store holds afterwards. A test that asserted the caller's intent would have
//! passed throughout the defect.

mod support;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use fs3_core::{BlobRef, Config, DatabaseConfig, Element, ElementKind, Embedder, Error, Span};
use fs3_daemon::scan::PARSER_VERSION;
use fs3_daemon::wiring::AppState;
use fs3_daemon::{enrich, runner};
use fs3_testkit::fakes::{FakeEmbedder, FakeSummarizer};
use serde_json::json;

const IDENTITY: &str = "git:github.com/fs3/oversize";

/// The cap the real models have, and the one the live failure named.
const CAP: usize = 8192;

/// A stack whose embedder refuses oversized inputs exactly as a hosted API
/// does, so a missing guard is a failing test rather than a hopeful comment.
async fn stack_using(
    label: &str,
    embedder: Arc<dyn Embedder>,
) -> (support::FreshDatabase, AppState) {
    let database = support::FreshDatabase::create(label).await;
    let config = Config {
        database: DatabaseConfig {
            url: database.url(),
        },
        ..Config::default()
    };
    let mut state = AppState::from_config(config).expect("the fake stack wires");
    fs3_store::migrate(&state.db).await.expect("migrates");
    state.embedder = embedder;
    (database, state)
}

async fn stack(label: &str) -> (support::FreshDatabase, AppState, Arc<FakeEmbedder>) {
    let embedder = Arc::new(FakeEmbedder {
        dimensions: fs3_store::EMBEDDING_DIMENSIONS,
        ..FakeEmbedder::capped(CAP)
    });
    let (database, state) = stack_using(label, embedder.clone()).await;
    (database, state, embedder)
}

#[derive(Debug)]
struct DenseCapEmbedder {
    calls: Mutex<Vec<Vec<String>>>,
    accepted: Mutex<Vec<String>>,
    bytes_per_token: usize,
    include_index: bool,
    always_reject: bool,
}

impl DenseCapEmbedder {
    fn new(bytes_per_token: usize, include_index: bool, always_reject: bool) -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            accepted: Mutex::new(Vec::new()),
            bytes_per_token,
            include_index,
            always_reject,
        }
    }

    fn accepted(&self) -> Vec<String> {
        self.accepted.lock().expect("accepted lock").clone()
    }
}

#[async_trait]
impl Embedder for DenseCapEmbedder {
    async fn embed(&self, texts: &[String]) -> fs3_core::Result<Vec<Vec<f32>>> {
        self.calls.lock().expect("calls lock").push(texts.to_vec());
        let rejected = if self.always_reject {
            texts.first().map(|text| (0, text))
        } else {
            texts.iter().enumerate().find(|(_, text)| {
                text.len().div_ceil(self.bytes_per_token) > self.max_input_tokens()
            })
        };
        if let Some((index, _)) = rejected {
            let detail = if self.include_index {
                format!(
                    "Invalid 'input[{index}]': maximum input length is {} tokens",
                    self.max_input_tokens()
                )
            } else {
                format!("maximum input length is {} tokens", self.max_input_tokens())
            };
            return Err(Error::InputTooLong {
                input_index: self.include_index.then_some(index),
                max_tokens: self.max_input_tokens(),
                detail,
            });
        }

        self.accepted
            .lock()
            .expect("accepted lock")
            .extend_from_slice(texts);
        Ok(texts
            .iter()
            .enumerate()
            .map(|(index, _)| {
                let mut vector = vec![0.0; fs3_store::EMBEDDING_DIMENSIONS];
                vector[index % fs3_store::EMBEDDING_DIMENSIONS] = 1.0;
                vector
            })
            .collect())
    }

    fn key(&self) -> String {
        format!("dense-cap@{}", self.bytes_per_token)
    }

    fn concurrency_ceiling(&self) -> usize {
        1
    }

    fn max_input_tokens(&self) -> usize {
        CAP
    }
}

/// A text far over the cap with position-bearing lines, so coverage checks can
/// locate every overlapping chunk unambiguously in the original.
fn oversized() -> String {
    (0..2_000)
        .map(|n| {
            format!("fn handler_{n:04}(request: Request) -> Response {{ dispatch(request) }}\n")
        })
        .collect()
}

/// Large enough that its bounded chunks exceed the aggregate request budget.
fn request_whale() -> String {
    (0..10_000)
        .map(|n| {
            format!("fn request_whale_{n:05}(input: Request) -> Response {{ dispatch(input) }}\n")
        })
        .collect()
}

/// An embed payload for `items`, with a root registered that HOLDS them.
async fn payload(state: &AppState, items: &[(String, String)]) -> serde_json::Value {
    support::hold(state, "oversize", items).await;
    json!({ "identity": IDENTITY, "source": "raw", "items": items })
}

async fn stored_chunks(state: &AppState, hash: &str) -> Vec<(i16, bool)> {
    sqlx::query_as(
        "SELECT chunk_no, truncated FROM embeddings_1024
          WHERE source_hash = $1 ORDER BY chunk_no",
    )
    .bind(hash)
    .fetch_all(&state.db)
    .await
    .expect("reading stored chunks")
}

fn assert_lossless_coverage(original: &str, chunks: &[String]) {
    assert!(chunks.len() > 1, "the fixture must actually split");
    let mut prior_start = 0;
    let mut prior_end = chunks[0].len();
    assert!(original.starts_with(&chunks[0]), "chunk zero is the head");

    for chunk in &chunks[1..] {
        let relative = original[prior_start + 1..]
            .find(chunk)
            .expect("every provider chunk comes from the original text");
        let start = prior_start + 1 + relative;
        assert!(
            start < prior_end,
            "adjacent chunks must overlap: prior ended at {prior_end}, next starts at {start}"
        );
        prior_start = start;
        prior_end = start + chunk.len();
    }

    assert_eq!(
        prior_end,
        original.len(),
        "the final chunk reaches the tail"
    );
}

#[tokio::test]
async fn an_oversized_input_arrives_as_lossless_under_cap_chunks() {
    let (database, state, embedder) = stack("oversize_under_cap").await;
    let text = oversized();
    let hash = fs3_core::content_hash(text.as_bytes());

    enrich::embed(
        &state,
        payload(&state, &[(hash.clone(), text.clone())]).await,
    )
    .await
    .expect("an oversized element must still be embeddable");

    assert_eq!(
        embedder.call_count(),
        1,
        "ordinary chunk expansion below the request budget stays one call"
    );

    let received = embedder.received();
    assert_lossless_coverage(&text, &received);
    assert!(
        received
            .iter()
            .all(|chunk| fs3_core::estimate_tokens(chunk) <= CAP),
        "every provider input must fit the model cap"
    );

    let stored = stored_chunks(&state, &hash).await;
    assert_eq!(stored.len(), received.len());
    assert_eq!(
        stored
            .iter()
            .map(|(chunk_no, _)| *chunk_no)
            .collect::<Vec<_>>(),
        (0..stored.len() as i16).collect::<Vec<_>>()
    );
    assert!(stored.iter().all(|(_, truncated)| !truncated));

    database.destroy(state.db.clone()).await;
}

#[tokio::test]
async fn expanded_chunks_are_split_into_budgeted_provider_calls_then_stored_together() {
    let (database, state, embedder) = stack("oversize_request_budget").await;
    let text = request_whale();
    let hash = fs3_core::content_hash(text.as_bytes());

    enrich::embed(
        &state,
        payload(&state, &[(hash.clone(), text.clone())]).await,
    )
    .await
    .expect("a request-sized whale must split across provider calls");

    let calls = embedder.calls.lock().expect("fake calls lock").clone();
    assert!(calls.len() > 1, "expanded inputs must exceed one request");
    for call in &calls {
        let tokens: usize = call
            .iter()
            .map(|chunk| fs3_core::estimate_tokens(chunk))
            .sum();
        assert!(
            tokens <= fs3_daemon::batch::TOKEN_BUDGET,
            "provider call spent {tokens} tokens against budget {}",
            fs3_daemon::batch::TOKEN_BUDGET
        );
    }

    let received: Vec<String> = calls.into_iter().flatten().collect();
    assert_lossless_coverage(&text, &received);
    assert_eq!(
        received
            .iter()
            .map(String::as_str)
            .collect::<std::collections::HashSet<_>>()
            .len(),
        received.len(),
        "every prepared chunk reaches exactly one provider call"
    );
    assert_eq!(
        stored_chunks(&state, &hash).await.len(),
        received.len(),
        "all sub-call vectors land in the one complete store write"
    );

    database.destroy(state.db.clone()).await;
}

#[tokio::test]
async fn a_later_provider_call_failure_stores_no_partial_chunk_set() {
    let (database, mut state, _embedder) = stack("oversize_request_atomic").await;
    let embedder = Arc::new(FakeEmbedder {
        dimensions: fs3_store::EMBEDDING_DIMENSIONS,
        max_input_tokens: CAP,
        ..FakeEmbedder::failing_after(1)
    });
    state.embedder = embedder.clone();
    let text = request_whale();
    let hash = fs3_core::content_hash(text.as_bytes());

    let failure = enrich::embed(&state, payload(&state, &[(hash.clone(), text)]).await)
        .await
        .expect_err("the fake fails the second provider call");

    assert!(failure.retryable, "a provider outage remains retryable");
    assert_eq!(
        embedder.call_count(),
        2,
        "the second sub-call is the failure"
    );
    assert!(
        stored_chunks(&state, &hash).await.is_empty(),
        "no prefix chunk set may become visible after a later call fails"
    );

    database.destroy(state.db.clone()).await;
}

#[tokio::test]
async fn an_element_within_the_cap_is_sent_verbatim_as_chunk_zero() {
    let (database, state, embedder) = stack("oversize_within").await;
    let text = "fn small() -> u8 { 7 }".to_string();
    let hash = fs3_core::content_hash(text.as_bytes());

    enrich::embed(
        &state,
        payload(&state, &[(hash.clone(), text.clone())]).await,
    )
    .await
    .expect("embeds");

    assert_eq!(embedder.received(), vec![text], "sent byte-identically");
    assert_eq!(stored_chunks(&state, &hash).await, vec![(0, false)]);

    database.destroy(state.db.clone()).await;
}

#[tokio::test]
async fn one_huge_element_and_its_small_batchmates_all_land() {
    let (database, state, embedder) = stack("oversize_batchmates").await;

    let huge = oversized();
    let huge_hash = fs3_core::content_hash(huge.as_bytes());
    let small: Vec<(String, String)> = (0..3)
        .map(|n| {
            let text = format!("fn small{n}() {{}}");
            (fs3_core::content_hash(text.as_bytes()), text)
        })
        .collect();

    for (n, item) in std::iter::once(&(huge_hash.clone(), huge.clone()))
        .chain(small.iter())
        .enumerate()
    {
        fs3_store::enqueue_job(
            &state.db,
            enrich::EMBED,
            &format!("embed:oversize:{n}"),
            &payload(&state, std::slice::from_ref(item)).await,
            Duration::ZERO,
        )
        .await
        .expect("enqueues");
    }

    let drained = runner::drain(&state, 1).await;
    assert_eq!(drained.failed, 0, "no job may fail: {drained:?}");
    assert_eq!(drained.completed, 4, "all four settle: {drained:?}");
    assert_eq!(embedder.call_count(), 1, "the claimed jobs still merge");

    let received = embedder.received();
    for (_, text) in &small {
        assert!(
            received.contains(text),
            "small batchmate must arrive verbatim"
        );
    }
    let huge_chunks: Vec<String> = received
        .into_iter()
        .filter(|text| !small.iter().any(|(_, small)| small == text))
        .collect();
    assert_lossless_coverage(&huge, &huge_chunks);
    assert_eq!(
        stored_chunks(&state, &huge_hash).await.len(),
        huge_chunks.len()
    );
    for (hash, _) in &small {
        assert_eq!(stored_chunks(&state, hash).await, vec![(0, false)]);
    }

    database.destroy(state.db.clone()).await;
}

#[tokio::test]
async fn cap_rejection_heals_dense_input_into_unique_chunks() {
    let embedder = Arc::new(DenseCapEmbedder::new(1, true, false));
    let (database, state) = stack_using("cap_heal", embedder.clone()).await;
    let text = oversized()[..20_872].to_string();
    let hash = fs3_core::content_hash(text.as_bytes());

    enrich::embed(
        &state,
        payload(&state, &[(hash.clone(), text.clone())]).await,
    )
    .await
    .expect("the dense input is re-split and accepted");

    let accepted = embedder.accepted();
    assert_lossless_coverage(&text, &accepted);
    assert!(
        accepted.iter().all(|chunk| chunk.len() <= CAP),
        "one-byte-per-token provider must receive only under-cap chunks"
    );
    let stored = stored_chunks(&state, &hash).await;
    assert_eq!(stored.len(), accepted.len());
    assert_eq!(
        stored
            .iter()
            .map(|(chunk_no, _)| *chunk_no)
            .collect::<Vec<_>>(),
        (0..stored.len() as i16).collect::<Vec<_>>()
    );

    database.destroy(state.db.clone()).await;
}

#[tokio::test]
async fn cap_rejection_without_index_bisects_before_healing() {
    let embedder = Arc::new(DenseCapEmbedder::new(1, false, false));
    let (database, state) = stack_using("cap_heal_bisect", embedder.clone()).await;
    let small = "small input".to_string();
    let dense = "!".repeat(20_872);
    let small_hash = fs3_core::content_hash(small.as_bytes());
    let dense_hash = fs3_core::content_hash(dense.as_bytes());
    let items = vec![(small_hash.clone(), small), (dense_hash.clone(), dense)];

    enrich::embed(&state, payload(&state, &items).await)
        .await
        .expect("bisection isolates the unnamed oversized input");

    assert_eq!(stored_chunks(&state, &small_hash).await.len(), 1);
    assert!(stored_chunks(&state, &dense_hash).await.len() > 1);
    assert!(
        embedder.calls.lock().expect("calls lock").len() >= 4,
        "the unnamed rejection must narrow the call before healing"
    );

    database.destroy(state.db.clone()).await;
}

#[tokio::test]
async fn cap_rejection_exhaustion_is_terminal_and_named() {
    let embedder = Arc::new(DenseCapEmbedder::new(1, true, true));
    let (database, state) = stack_using("cap_heal_exhausted", embedder).await;
    let text = "!".repeat(20_872);
    let hash = fs3_core::content_hash(text.as_bytes());
    let dedupe_key = "embed:oversize:cap-heal-exhausted";

    fs3_store::enqueue_job(
        &state.db,
        enrich::EMBED,
        dedupe_key,
        &payload(&state, &[(hash.clone(), text.clone())]).await,
        Duration::ZERO,
    )
    .await
    .expect("enqueues");

    let drained = runner::drain(&state, 1).await;
    assert_eq!(drained.failed, 1, "exhaustion fails once: {drained:?}");
    let (job_state, terminal, message): (String, bool, Option<String>) =
        sqlx::query_as("SELECT state, terminal, last_error FROM jobs WHERE dedupe_key = $1")
            .bind(dedupe_key)
            .fetch_one(&state.db)
            .await
            .expect("reads failed job");
    assert_eq!(job_state, "failed");
    assert!(terminal, "heal exhaustion cannot become boot-loop work");
    let message = message.expect("failure is named");
    assert!(message.contains(&hash), "item identity missing: {message}");
    assert!(
        message.contains(&text.len().to_string()),
        "byte length missing: {message}"
    );
    assert!(
        message.contains("7500 bytes/7500 tokens"),
        "ratio missing: {message}"
    );

    let swept = fs3_store::requeue_failed(&state.db, &[enrich::SUMMARIZE, enrich::EMBED])
        .await
        .expect("runs boot sweep");
    assert_eq!(swept, 0, "terminal exhaustion must stay failed");

    database.destroy(state.db.clone()).await;
}

#[tokio::test]
async fn a_job_that_failed_before_chunking_is_requeued_and_lands_all_chunks() {
    let (database, state, embedder) = stack("oversize_recovery").await;
    let text = oversized();
    let hash = fs3_core::content_hash(text.as_bytes());

    fs3_store::enqueue_job(
        &state.db,
        enrich::EMBED,
        "embed:oversize:recovered",
        &payload(&state, &[(hash.clone(), text.clone())]).await,
        Duration::ZERO,
    )
    .await
    .expect("enqueues");

    let job = fs3_store::claim_job(&state.db, &[enrich::EMBED])
        .await
        .expect("claims")
        .expect("a job is ready");
    fs3_store::fail_job(
        &state.db,
        job.id,
        "provider_failed Invalid 'input[0]': maximum input length is 8192 tokens",
        false,
    )
    .await
    .expect("fails");

    assert_eq!(embedder.call_count(), 0);
    assert!(stored_chunks(&state, &hash).await.is_empty());
    let swept = fs3_store::requeue_failed(&state.db, &[enrich::SUMMARIZE, enrich::EMBED])
        .await
        .expect("requeues");
    assert_eq!(swept, 1, "the failed embed job is revivable");

    let drained = runner::drain(&state, 1).await;
    assert_eq!(drained.completed, 1, "and now it lands: {drained:?}");
    let received = embedder.received();
    assert_lossless_coverage(&text, &received);
    assert_eq!(stored_chunks(&state, &hash).await.len(), received.len());

    database.destroy(state.db.clone()).await;
}

/// A job that can never succeed is NOT requeued.
///
/// Without this, the sweep would wake an unreadable payload on every single
/// boot — an unbounded, permanent trickle of claims that can only fail again.
#[tokio::test]
async fn a_terminal_failure_is_left_where_it_is() {
    let (database, state, _embedder) = stack("oversize_terminal").await;

    fs3_store::enqueue_job(
        &state.db,
        enrich::EMBED,
        "embed:oversize:broken",
        &json!({ "this": "is not an embed payload" }),
        Duration::ZERO,
    )
    .await
    .expect("enqueues");

    // The runner fails an unreadable payload terminally and without a retry.
    let drained = runner::drain(&state, 1).await;
    assert_eq!(drained.failed, 1, "a malformed payload fails: {drained:?}");

    let swept = fs3_store::requeue_failed(&state.db, &[enrich::SUMMARIZE, enrich::EMBED])
        .await
        .expect("requeues");
    assert_eq!(swept, 0, "a defect must not be woken by the sweep");

    let pool = state.db.clone();
    database.destroy(pool).await;
}

// ── The summarize side ──────────────────────────────────────────────────────

/// One element, its file, and a worktree holding it — everything
/// `enrich::summarize` checks before it spends anything.
async fn seed_element(state: &AppState, body: &str) -> Element {
    let child = Element::new(
        ElementKind::Function,
        "function_item",
        "handler",
        "src/big.rs::handler",
        Span::new(3, 5),
        body,
    )
    .with_sibling_order(0);
    let file = Element::new(
        ElementKind::File,
        "file",
        "big.rs",
        "src/big.rs",
        Span::new(1, 9),
        "// src/big.rs\n",
    )
    .with_children(vec![child.clone()]);

    let blob = BlobRef::new(format!("{:040x}", 0x5eed_u64)).expect("a blob sha");
    let worktree = fs3_store::register_worktree(
        &state.db,
        &fs3_core::RepoIdentity::from_path(std::path::Path::new("/srv/oversize")),
        "/srv/oversize",
        Some("main"),
    )
    .await
    .expect("registers");
    fs3_store::sync_worktree_files(
        &state.db,
        worktree,
        &[("src/big.rs".to_string(), blob.clone())],
    )
    .await
    .expect("maps the file");
    fs3_store::upsert_element_tree(&state.db, &blob, PARSER_VERSION, &file, |element| {
        element.kind != ElementKind::File
    })
    .await
    .expect("stores the parse");

    child
}

/// The same cliff, on the other provider.
///
/// A chat model's window is far larger than an embedding model's per-input
/// cap, but "larger" is not "absent": one element can be a whole generated
/// file. Mutation check: remove the guard in `enrich::summarize` and the
/// capped fake refuses the call, exactly as a server out of context does.
#[tokio::test]
async fn an_oversized_element_is_summarised_from_a_prefix_and_says_so() {
    let (database, mut state, _embedder) = stack("oversize_summarize").await;

    // Far under the embedding cap in spirit, far over this model's window: the
    // point is that the number is different, not that the cliff is.
    let summarizer = Arc::new(FakeSummarizer::capped(4_000));
    state.summarizer = summarizer.clone();

    let body = oversized();
    let element = seed_element(&state, &body).await;
    let raw_hash = element.raw_hash().to_string();

    enrich::summarize(
        &state,
        json!({
            "identity": IDENTITY,
            "raw_hash": raw_hash,
            "element": element,
        }),
    )
    .await
    .expect("an oversized element must still be summarisable");

    let received = summarizer.received();
    assert_eq!(received.len(), 1, "one prompt was built");
    assert!(
        fs3_core::estimate_tokens(&received[0]) <= 4_000,
        "the prompt must fit the model's window"
    );
    assert!(
        body.starts_with(&received[0]) && received[0].len() < body.len(),
        "and must be a genuine prefix of the element"
    );

    // The honesty half: a summary written from a prefix must not be
    // indistinguishable from a summary of the whole element.
    let stored =
        fs3_store::get_smart_content(&state.db, &raw_hash, &state.summarizer_key(IDENTITY))
            .await
            .expect("reads")
            .expect("the summary was stored");
    assert_eq!(
        stored.extras.get("truncated_input"),
        Some(&json!(true)),
        "the summary must record that it only saw part of its element"
    );

    let pool = state.db.clone();
    database.destroy(pool).await;
}

/// An element that fits leaves no marker — the other half of the mutation
/// check for the summarize guard.
#[tokio::test]
async fn an_element_within_the_prompt_budget_is_summarised_whole() {
    let (database, mut state, _embedder) = stack("oversize_summarize_small").await;
    let summarizer = Arc::new(FakeSummarizer::capped(4_000));
    state.summarizer = summarizer.clone();

    let body = "fn handler() -> u8 { 7 }".to_string();
    let element = seed_element(&state, &body).await;
    let raw_hash = element.raw_hash().to_string();

    enrich::summarize(
        &state,
        json!({
            "identity": IDENTITY,
            "raw_hash": raw_hash,
            "element": element,
        }),
    )
    .await
    .expect("summarises");

    assert_eq!(summarizer.received(), vec![body], "sent verbatim");
    let stored =
        fs3_store::get_smart_content(&state.db, &raw_hash, &state.summarizer_key(IDENTITY))
            .await
            .expect("reads")
            .expect("stored");
    assert_eq!(
        stored.extras.get("truncated_input"),
        None,
        "an element that fits must carry no truncation marker"
    );

    let pool = state.db.clone();
    database.destroy(pool).await;
}
