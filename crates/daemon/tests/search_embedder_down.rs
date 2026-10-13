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

#[tokio::test]
async fn a_search_with_an_unanswering_embedder_returns_keyword_hits_within_the_bound() {
    let database = support::FreshDatabase::create("embedder_down").await;
    let config = Config {
        database: DatabaseConfig {
            url: database.url(),
        },
        ..Config::default()
    };
    let mut state = AppState::from_config(config).expect("wires");
    fs3_store::migrate(&state.db).await.expect("migrates");
    state.embedder = Arc::new(NeverAnswers);
    state.query_embed_bound = Duration::from_millis(300);

    let phrase = "quetzal retry ledger";
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

    let started = std::time::Instant::now();
    let outcome = tokio::time::timeout(
        Duration::from_secs(5),
        search(
            &state,
            &SearchRequest {
                q: phrase.to_string(),
                limit: Some(10),
                ..SearchRequest::default()
            },
            &Scope::unscoped(),
        ),
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
