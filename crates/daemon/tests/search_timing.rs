//! Every search says where its time went, and status keeps percentiles.
//!
//! Before this, a search logged its split only past five seconds, so the
//! steady four-second searches of 2026-10-14 left no record at all.

mod support;

use std::sync::Arc;

use fs3_core::{Config, DatabaseConfig, Element, ElementKind, Span};
use fs3_daemon::wiring::AppState;
use fs3_store::{NewEmbedding, SourceKind};
use fs3_testkit::fakes::FakeEmbedder;

const PHRASE: &str = "quetzal retry ledger";

async fn seeded(label: &str) -> (support::FreshDatabase, AppState) {
    let database = support::FreshDatabase::create(label).await;
    let config = Config {
        database: DatabaseConfig {
            url: database.url(),
        },
        ..Config::default()
    };
    let mut state = AppState::from_config(config).expect("wires");
    fs3_store::migrate(&state.db).await.expect("migrates");
    state.embedder = Arc::new(FakeEmbedder {
        dimensions: fs3_store::EMBEDDING_DIMENSIONS,
        ..FakeEmbedder::default()
    });

    let child = Element::new(
        ElementKind::Function,
        "function_item",
        "ledger",
        "src/lib.rs::ledger",
        Span::new(1, 1),
        format!("fn ledger() {{ /* {PHRASE} */ }}"),
    );
    let root = Element::new(
        ElementKind::File,
        "rust",
        "src/lib.rs",
        "src/lib.rs",
        Span::new(1, 1),
        child.raw_text.clone(),
    )
    .with_children(vec![child.clone()]);
    let blob = fs3_core::BlobRef::new("c".repeat(40)).unwrap();
    fs3_store::upsert_element_tree(&state.db, &blob, "test@1", &root, |_| true)
        .await
        .unwrap();
    let vector = vec![0.1; fs3_store::EMBEDDING_DIMENSIONS];
    fs3_store::put_embeddings(
        &state.db,
        &state.embedder_key(""),
        &[NewEmbedding {
            chunk_no: 0,
            source_hash: child.raw_hash(),
            source_kind: SourceKind::Raw,
            vector: &vector,
            truncated: false,
        }],
    )
    .await
    .unwrap();
    (database, state)
}

#[tokio::test]
async fn a_search_carries_its_timing_and_status_reports_percentiles() {
    let (database, state) = seeded("search_timing").await;
    assert_eq!(
        fs3_daemon::status::report(&state, false)
            .await
            .expect("status answers")
            .search_latency,
        None,
        "nothing to report before the first search"
    );

    let auth = support::auth("search-timing");
    let base = support::spawn(fs3_daemon::http::router(state.clone(), auth.auth)).await;
    for _ in 0..3 {
        let envelope: serde_json::Value = reqwest::Client::new()
            .get(format!("{base}/search"))
            .bearer_auth(&auth.key)
            .query(&[("q", PHRASE), ("limit", "5")])
            .send()
            .await
            .expect("the daemon answers")
            .json()
            .await
            .expect("an envelope");
        assert_eq!(envelope["ok"], true, "{envelope}");

        let timing = &envelope["meta"]["timing"];
        for stage in [
            "total_ms",
            "guard_ms",
            "scope_ms",
            "embed_ms",
            "store_ms",
            "semantic_ms",
            "lexical_ms",
            "rank_ms",
            "agents_ms",
        ] {
            assert!(timing[stage].is_u64(), "{stage} is reported: {timing}");
        }
        let passes = timing["semantic_pass_ms"]
            .as_array()
            .expect("one entry per candidate pass");
        assert_eq!(
            serde_json::json!(passes.len()),
            envelope["meta"]["passes"],
            "{timing}"
        );
        assert!(timing["candidates"].as_i64() > Some(0), "{timing}");
        let total = timing["total_ms"].as_u64().unwrap();
        assert!(
            timing["store_ms"].as_u64().unwrap() <= total
                && timing["semantic_ms"].as_u64().unwrap() <= total,
            "a stage cannot outlast the search: {timing}"
        );
    }

    let latency = fs3_daemon::status::report(&state, false)
        .await
        .expect("status answers")
        .search_latency
        .expect("three searches were recorded");
    assert_eq!(latency.searches, 3);
    assert_eq!(latency.stages[0].stage, "total");
    assert!(latency.stages.iter().any(|row| row.stage == "semantic"));
    assert!(
        latency.stages.iter().all(|row| row.stage != "wide"),
        "an unscoped search runs no wide legs, so reports none"
    );
    assert!(!latency.slowest_stage.is_empty());

    database.destroy(state.db).await;
}
