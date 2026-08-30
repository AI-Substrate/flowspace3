//! Verbatim lexical hits stay ahead of a stronger semantic distractor.
//!
//! Mutation check: removing the lexical store call makes the distractor rank
//! first and this test fail before any channel assertion is reached.

mod support;

use std::sync::Arc;

use fs3_core::views::search::SearchChannel;
use fs3_core::{Config, DatabaseConfig, Element, ElementKind, Span};
use fs3_daemon::scope::Scope;
use fs3_daemon::search::{SearchRequest, search};
use fs3_daemon::wiring::AppState;
use fs3_store::{NewEmbedding, SourceKind};
use fs3_testkit::fakes::FakeEmbedder;

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
async fn leopon_verbatim_phrase_hits_are_all_pinned_before_semantic_noise() {
    let database = support::FreshDatabase::create("lexical_anchor").await;
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

    let phrase = "leopon verbatim phrase anchor";
    let children = vec![
        function(
            "anchor_one",
            &format!("fn anchor_one() {{ /* {phrase} */ }}"),
            3,
        ),
        function(
            "anchor_two",
            &format!("fn anchor_two() {{ let x = \"{phrase}\"; }}"),
            7,
        ),
        function(
            "anchor_three",
            &format!("fn anchor_three() {{ panic!(\"{phrase}\") }}"),
            11,
        ),
        function("semantic_distractor", "fn semantic_distractor() {}", 15),
    ];
    let root = Element::new(
        ElementKind::File,
        "rust",
        "src/lib.rs",
        "src/lib.rs",
        Span::new(1, 20),
        children
            .iter()
            .map(|child| child.raw_text.as_str())
            .collect::<Vec<_>>()
            .join("\n"),
    )
    .with_children(children.clone());
    let blob = fs3_core::BlobRef::new("a".repeat(40)).unwrap();
    fs3_store::upsert_element_tree(&state.db, &blob, "test@1", &root, |_| true)
        .await
        .unwrap();

    let query_vector = state
        .embedder
        .embed(&[phrase.to_string()])
        .await
        .unwrap()
        .remove(0);
    let weaker = vec![0.1; fs3_store::EMBEDDING_DIMENSIONS];
    let model_key = state.embedder_key("");
    fs3_store::put_embeddings(
        &state.db,
        &model_key,
        &[
            NewEmbedding {
                source_hash: children[3].raw_hash(),
                source_kind: SourceKind::Raw,
                vector: &query_vector,
                truncated: false,
            },
            NewEmbedding {
                source_hash: children[0].raw_hash(),
                source_kind: SourceKind::Raw,
                vector: &weaker,
                truncated: false,
            },
        ],
    )
    .await
    .unwrap();

    let outcome = search(
        &state,
        &SearchRequest {
            q: phrase.to_string(),
            limit: Some(10),
            ..SearchRequest::default()
        },
        &Scope::unscoped(),
    )
    .await
    .expect("fused search answers");

    let first_three = &outcome.results[..3];
    assert!(
        first_three
            .iter()
            .all(|hit| hit.name.starts_with("anchor_")),
        "all verbatim elements must precede the exact-vector distractor: {:#?}",
        outcome.results
    );
    assert!(
        first_three
            .iter()
            .all(|hit| matches!(hit.channel, SearchChannel::Lexical | SearchChannel::Both))
    );
    assert_eq!(first_three[0].channel, SearchChannel::Both);
    assert_eq!(first_three[0].score, 1.0, "both keeps the lexical score");
    assert_eq!(outcome.results[3].name, "semantic_distractor");
    assert_eq!(outcome.results[3].channel, SearchChannel::Semantic);

    let pool = state.db.clone();
    database.destroy(pool).await;
}
