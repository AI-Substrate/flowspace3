//! A search answers from keywords when the embedder does not answer (DL-027).
//!
//! On 2026-10-11 the embedder could not authenticate (DNS for its token
//! endpoint failed) and every search hung, silently, until the client gave up,
//! while `status` answered in under a second.

mod support;

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use fs3_core::ports::Embedder;
use fs3_core::{Config, DatabaseConfig, Element, ElementKind, Result, Span};
use fs3_daemon::scope::Scope;
use fs3_daemon::search::{SearchRequest, search};
use fs3_daemon::wiring::AppState;
use fs3_store::{NewEmbedding, SourceKind};

/// An embedder whose every call hangs, like a token fetch that never resolves.
#[derive(Debug)]
struct NeverAnswers;

#[async_trait]
impl Embedder for NeverAnswers {
    async fn embed(&self, _texts: &[String]) -> Result<Vec<Vec<f32>>> {
        std::future::pending().await
    }

    fn key(&self) -> String {
        format!("never@{}", fs3_store::EMBEDDING_DIMENSIONS)
    }

    fn concurrency_ceiling(&self) -> usize {
        1
    }

    fn max_input_tokens(&self) -> usize {
        8192
    }
}

fn function(name: &str, body: &str, line: u32) -> Element {
    Element::new(
        ElementKind::Function,
        "function_item",
        name,
        format!("src/lib.rs::{name}"),
        Span::new(line, line),
        body,
    )
}

/// A daemon state whose index holds one keyword hit for [`PHRASE`], with the
/// given embedder answering queries.
async fn seeded(label: &str, embedder: Arc<dyn Embedder>) -> (support::FreshDatabase, AppState) {
    let database = support::FreshDatabase::create(label).await;
    let config = Config {
        database: DatabaseConfig {
            url: database.url(),
        },
        ..Config::default()
    };
    let mut state = AppState::from_config(config).expect("wires");
    fs3_store::migrate(&state.db).await.expect("migrates");
    state.embedder = embedder;
    state.query_embed_bound = Duration::from_millis(300);

    let phrase = PHRASE;
    let children = vec![
        function("ledger", &format!("fn ledger() {{ /* {phrase} */ }}"), 3),
        function("unrelated", "fn unrelated() {}", 7),
    ];
    let root = Element::new(
        ElementKind::File,
        "rust",
        "src/lib.rs",
        "src/lib.rs",
        Span::new(1, 10),
        children
            .iter()
            .map(|child| child.raw_text.as_str())
            .collect::<Vec<_>>()
            .join("\n"),
    )
    .with_children(children.clone());
    let blob = fs3_core::BlobRef::new("b".repeat(40)).unwrap();
    fs3_store::upsert_element_tree(&state.db, &blob, "test@1", &root, |_| true)
        .await
        .unwrap();
    // The index is not empty for this model: it was built while the embedder
    // still answered.
    let vector = vec![0.1; fs3_store::EMBEDDING_DIMENSIONS];
    fs3_store::put_embeddings(
        &state.db,
        &state.embedder_key(""),
        &[NewEmbedding {
            chunk_no: 0,
            source_hash: children[1].raw_hash(),
            source_kind: SourceKind::Raw,
            vector: &vector,
            truncated: false,
        }],
    )
    .await
    .unwrap();

    (database, state)
}

fn request() -> SearchRequest {
    SearchRequest {
        q: PHRASE.to_string(),
        limit: Some(10),
        ..SearchRequest::default()
    }
}

const PHRASE: &str = "quetzal retry ledger";

#[tokio::test]
async fn a_search_with_an_unanswering_embedder_returns_keyword_hits_within_the_bound() {
    let (database, state) = seeded("embedder_down", Arc::new(NeverAnswers)).await;
    let started = std::time::Instant::now();
    let outcome = tokio::time::timeout(
        Duration::from_secs(5),
        search(&state, &request(), &Scope::unscoped()),
    )
    .await
    .expect("the search answers instead of waiting on the embedder")
    .expect("the search succeeds");

    assert!(
        started.elapsed() < Duration::from_secs(2),
        "answered soon after the 300 ms bound, took {:?}",
        started.elapsed()
    );
    assert!(
        outcome
            .results
            .iter()
            .any(|hit| hit.address.ends_with("src/lib.rs::ledger")),
        "the keyword hit is returned: {:?}",
        outcome
            .results
            .iter()
            .map(|hit| &hit.address)
            .collect::<Vec<_>>()
    );
    let degraded = outcome.degraded.expect("the answer says it is degraded");
    assert_eq!(degraded.because, "embedder_unavailable");
    assert!(
        degraded.detail.contains("did not answer within"),
        "{}",
        degraded.detail
    );
    assert!(degraded.since.is_some(), "it says since when");

    let failing = state
        .embedder_health
        .failing()
        .expect("status can say the embedder is failing");
    assert_eq!(failing.failures, 1);

    let report = fs3_daemon::status::report(&state, false)
        .await
        .expect("status answers");
    assert_eq!(report.embedder_failing, Some(failing));

    database.destroy(state.db.clone()).await;
}

/// An embedder slower than the bound (a cold credential: 15 s after a restart
/// on 2026-10-13). The first search answers from keywords, and the call it
/// started still finishes, so the next search is ranked by meaning.
#[tokio::test]
async fn a_slow_first_query_embed_warms_up_for_the_next_search() {
    let (database, state) = seeded(
        "embedder_slow",
        Arc::new(AnswersAfter(Duration::from_millis(700))),
    )
    .await;

    let first = search(&state, &request(), &Scope::unscoped())
        .await
        .expect("the first search answers");
    assert!(first.degraded.is_some(), "the first search did not wait");

    tokio::time::sleep(Duration::from_millis(1_000)).await;
    assert!(
        state.embedder_health.failing().is_none(),
        "the late answer clears the failure"
    );
    let second = search(&state, &request(), &Scope::unscoped())
        .await
        .expect("the second search answers");
    assert!(
        second.degraded.is_none(),
        "the late vector was kept: {:?}",
        second.degraded
    );

    database.destroy(state.db.clone()).await;
}

/// While query embeds are piling up unanswered, a search stops adding to them
/// and answers from keywords at once.
#[tokio::test]
async fn a_search_does_not_wait_while_query_embeds_are_piling_up() {
    let (database, state) = seeded("embedder_piling", Arc::new(NeverAnswers)).await;
    for n in 0..4 {
        let mut request = request();
        request.q = format!("{PHRASE} {n}");
        search(&state, &request, &Scope::unscoped())
            .await
            .expect("answers from keywords");
    }

    let started = std::time::Instant::now();
    let outcome = search(&state, &request(), &Scope::unscoped())
        .await
        .expect("answers from keywords");
    assert!(
        started.elapsed() < Duration::from_millis(250),
        "did not wait out the bound again: {:?}",
        started.elapsed()
    );
    let degraded = outcome.degraded.expect("still degraded");
    assert!(
        degraded.detail.contains("still unanswered"),
        "{}",
        degraded.detail
    );

    database.destroy(state.db.clone()).await;
}

/// An embedder that answers, but only after a delay.
#[derive(Debug)]
struct AnswersAfter(Duration);

#[async_trait]
impl Embedder for AnswersAfter {
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        tokio::time::sleep(self.0).await;
        Ok(texts
            .iter()
            .map(|_| vec![0.1; fs3_store::EMBEDDING_DIMENSIONS])
            .collect())
    }

    fn key(&self) -> String {
        format!("never@{}", fs3_store::EMBEDDING_DIMENSIONS)
    }

    fn concurrency_ceiling(&self) -> usize {
        1
    }

    fn max_input_tokens(&self) -> usize {
        8192
    }
}
