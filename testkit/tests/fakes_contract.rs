//! Exemplar: the port contract tier.
//!
//! `embedder_contract` is the *same function* the real provider runs behind
//! `#[ignore]` (see `providers/tests/openai_contract.rs`). Copy this shape when
//! a new port implementation lands.

use fs3_core::{Embedder, Summarizer};
use fs3_testkit::{FakeEmbedder, FakeSummarizer, embedder_contract, summarizer_contract};
use std::sync::Arc;

#[tokio::test]
async fn fake_embedder_honours_the_embedder_contract() {
    embedder_contract(&FakeEmbedder::default()).await;
}

#[tokio::test]
async fn fake_summarizer_honours_the_summarizer_contract() {
    summarizer_contract(&FakeSummarizer::default()).await;
}

/// The contract harness must work through the same `dyn` seam the composition
/// root uses — otherwise it proves something the daemon never runs.
#[tokio::test]
async fn contract_holds_through_the_dyn_port() {
    let embedder: Arc<dyn Embedder> = Arc::new(FakeEmbedder::default());
    embedder_contract(embedder.as_ref()).await;

    let summarizer: Arc<dyn Summarizer> = Arc::new(FakeSummarizer::default());
    summarizer_contract(summarizer.as_ref()).await;
}

/// Determinism is the property that makes the fake usable as a *provider*
/// rather than a stub: indexing twice must not churn every vector.
#[tokio::test]
async fn embedding_is_deterministic_across_separate_instances() {
    let texts = vec!["fn needs_summary() -> bool".to_string()];

    let first = FakeEmbedder::default().embed(&texts).await.unwrap();
    let second = FakeEmbedder::default().embed(&texts).await.unwrap();

    assert_eq!(first, second, "same text must yield the same vector");
}

#[tokio::test]
async fn calls_are_recorded_in_order() {
    let embedder = FakeEmbedder::default();
    embedder.embed(&["a".to_string()]).await.unwrap();
    embedder
        .embed(&["b".to_string(), "c".to_string()])
        .await
        .unwrap();

    assert_eq!(embedder.call_count(), 2);
    assert_eq!(
        *embedder.calls.lock().unwrap(),
        vec![
            vec!["a".to_string()],
            vec!["b".to_string(), "c".to_string()]
        ]
    );
}

#[tokio::test]
async fn failure_injection_needs_no_mocking_framework() {
    let embedder = FakeEmbedder::failing_after(1);

    assert!(embedder.embed(&["ok".to_string()]).await.is_ok());
    let err = embedder
        .embed(&["boom".to_string()])
        .await
        .expect_err("second call was configured to fail");
    assert!(err.to_string().contains("injected failure"), "got {err}");

    let summarizer = FakeSummarizer::failing_after(0);
    assert!(
        summarizer
            .summarize(&fs3_testkit::contract::sample_element())
            .await
            .is_err()
    );
}
