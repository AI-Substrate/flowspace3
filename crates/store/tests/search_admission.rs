//! Search-admission parity across every caller filter and the shared-hash shapes
//! that make existential admission easy to change accidentally.

mod support;

use fs3_core::{
    BlobRef, Conversation, ConversationId, Element, ElementKind, RepoIdentity, Span, Summary, Turn,
    TurnRole, TurnSource, content_hash,
};
use fs3_store::{
    EMBEDDING_DIMENSIONS, NewEmbedding, PgPool, SearchFilters, SourceKind, append_turns,
    put_embeddings, put_smart_content, register_worktree, search_elements, sync_worktree_files,
    upsert_conversation, upsert_element_tree,
};
use fs3_testkit::FreshDatabase;
use serde_json::{Map, Value, json};
use support::{PARSER_VERSION, unique_blob};

const EMBEDDER: &str = "search-admission-fixture@1024";
const SUMMARIZER: &str = "search-admission-summary@v1";
const CONVERSATION: &str = "6ba7b810-9dad-11d1-80b4-00c04fd43017";

struct StoredElement {
    blob: BlobRef,
    raw_hash: String,
}

fn vector(y: f32) -> Vec<f32> {
    let mut vector = vec![0.0; EMBEDDING_DIMENSIONS];
    vector[0] = 1.0;
    vector[1] = y;
    vector
}

async fn store_element(
    pool: &PgPool,
    path: &str,
    name: &str,
    kind: ElementKind,
    body: &str,
) -> StoredElement {
    let blob = unique_blob();
    let child = Element::new(
        kind,
        "search_admission_fixture",
        name,
        format!("{path}::{name}"),
        Span::new(1, 1),
        body,
    );
    let raw_hash = child.raw_hash().to_string();
    let root = Element::new(
        ElementKind::File,
        "fixture",
        path,
        path,
        Span::new(1, 1),
        body,
    )
    .with_children(vec![child]);
    upsert_element_tree(pool, &blob, PARSER_VERSION, &root, |element| {
        element.kind != ElementKind::File
    })
    .await
    .expect("store parity element");
    StoredElement { blob, raw_hash }
}

fn turn(turn_no: u32, body: &str) -> Turn {
    Turn {
        turn_no,
        role: if turn_no % 2 == 1 {
            TurnRole::Human
        } else {
            TurnRole::Agent
        },
        source: TurnSource::Peer,
        head_sha: None,
        at: "2026-09-02T00:00:00Z".to_string(),
        body: body.to_string(),
        items: Vec::new(),
    }
}

fn compare_golden(expected: &Value, actual: &Value) -> Result<(), String> {
    let expected = expected
        .as_object()
        .ok_or_else(|| "golden root is not an object".to_string())?;
    let actual = actual
        .as_object()
        .ok_or_else(|| "actual root is not an object".to_string())?;
    if expected.len() != actual.len() {
        return Err(format!(
            "golden has {} cases; actual has {}",
            expected.len(),
            actual.len()
        ));
    }

    for (case, actual_hits) in actual {
        let expected_hits = expected
            .get(case)
            .and_then(Value::as_array)
            .ok_or_else(|| format!("golden has no array for {case}"))?;
        let actual_hits = actual_hits
            .as_array()
            .ok_or_else(|| format!("actual {case} is not an array"))?;
        if expected_hits.len() != actual_hits.len() {
            return Err(format!(
                "{case}: expected {} hits, got {}",
                expected_hits.len(),
                actual_hits.len()
            ));
        }
        for (index, (expected, actual)) in expected_hits.iter().zip(actual_hits).enumerate() {
            let expected_address = expected["address"].as_str();
            let actual_address = actual["address"].as_str();
            if expected_address != actual_address {
                return Err(format!(
                    "{case}[{index}]: expected address {expected_address:?}, got {actual_address:?}"
                ));
            }
            let expected_score = expected["score"]
                .as_f64()
                .ok_or_else(|| format!("{case}[{index}]: expected score is not numeric"))?;
            let actual_score = actual["score"]
                .as_f64()
                .ok_or_else(|| format!("{case}[{index}]: actual score is not numeric"))?;
            if (expected_score - actual_score).abs() > 1e-6 {
                return Err(format!(
                    "{case}[{index}]: expected score {expected_score}, got {actual_score}"
                ));
            }
        }
    }
    Ok(())
}

#[tokio::test]
async fn search_parity_matches_old_query_golden() {
    let database =
        FreshDatabase::create_from(&fs3_testkit::test_database_url(), "search-admission")
            .await
            .expect("create migrated search-admission database");
    let pool = database.pool().await;

    let alpha = RepoIdentity::from_remote_parts(Some("github.com"), "fixtures/alpha").unwrap();
    let beta = RepoIdentity::from_remote_parts(Some("github.com"), "fixtures/beta").unwrap();
    let alpha_root = "/fixtures/alpha";
    let alpha_worktree = register_worktree(&pool, &alpha, alpha_root, Some("main"))
        .await
        .unwrap();
    let beta_worktree = register_worktree(&pool, &beta, "/fixtures/beta", Some("main"))
        .await
        .unwrap();

    // Three eligible elements share one raw hash. Admission is existential: the
    // vector is one candidate regardless of how many elements carry the body.
    let shared_body = "fn shared_raw_admission() { preserve_existential_multiplicity(); }";
    let mut alpha_paths = Vec::new();
    let mut shared_raw_hash = None;
    for suffix in ["a", "b", "c"] {
        let path = format!("src/shared/raw-{suffix}.rs");
        let stored = store_element(
            &pool,
            &path,
            "shared_raw_admission",
            ElementKind::Function,
            shared_body,
        )
        .await;
        shared_raw_hash.get_or_insert_with(|| stored.raw_hash.clone());
        assert_eq!(shared_raw_hash.as_deref(), Some(stored.raw_hash.as_str()));
        alpha_paths.push((path, stored.blob));
    }

    // Two distinct raw hashes share one summary text hash. The smart candidate
    // is still one vector, and its representative remains deterministic.
    let smart_a = store_element(
        &pool,
        "src/shared/smart-a.rs",
        "smart_a",
        ElementKind::Function,
        "fn smart_a() { choose_the_first_eligible_mapping(); }",
    )
    .await;
    let smart_b = store_element(
        &pool,
        "src/shared/smart-b.rs",
        "smart_b",
        ElementKind::Function,
        "fn smart_b() { choose_the_second_eligible_mapping(); }",
    )
    .await;
    alpha_paths.push(("src/shared/smart-a.rs".to_string(), smart_a.blob));
    alpha_paths.push(("src/shared/smart-b.rs".to_string(), smart_b.blob));
    sync_worktree_files(&pool, alpha_worktree, &alpha_paths)
        .await
        .unwrap();

    let beta_element = store_element(
        &pool,
        "src/foreign.rs",
        "foreign_near_match",
        ElementKind::Function,
        "fn foreign_near_match() { should_be_filtered_by_repo(); }",
    )
    .await;
    sync_worktree_files(
        &pool,
        beta_worktree,
        &[("src/foreign.rs".to_string(), beta_element.blob)],
    )
    .await
    .unwrap();

    let code_summary = Summary {
        text: "Chooses one eligible smart mapping without multiplying candidates.".to_string(),
        tags: vec!["admission".to_string()],
        ..Summary::default()
    };
    put_smart_content(&pool, &smart_a.raw_hash, SUMMARIZER, &code_summary)
        .await
        .unwrap();
    put_smart_content(&pool, &smart_b.raw_hash, SUMMARIZER, &code_summary)
        .await
        .unwrap();

    // The conversation fixture repeats both dangerous shapes inside one exact
    // conversation scope: three equal turn bodies, then two raw bodies sharing
    // one summary.
    let conversation_id = ConversationId::new(CONVERSATION).unwrap();
    upsert_conversation(
        &pool,
        &Conversation {
            guid: conversation_id.clone(),
            repo_identity: Some(alpha.key().to_string()),
            worktree: Some(alpha_root.to_string()),
            base_sha: None,
            title: Some("search admission parity".to_string()),
            started_at: "2026-09-02T00:00:00Z".to_string(),
            parent: None,
        },
    )
    .await
    .unwrap();
    let conversation_shared = "the same conversation turn body exercises shared raw admission";
    let appended = append_turns(
        &pool,
        &conversation_id,
        &[
            turn(1, conversation_shared),
            turn(2, conversation_shared),
            turn(3, conversation_shared),
            turn(4, "first raw conversation body for one shared summary"),
            turn(5, "second raw conversation body for one shared summary"),
        ],
        |_| false,
    )
    .await
    .unwrap();
    assert_eq!(appended.accepted.len(), 5);
    let conversation_summary = Summary {
        text: "Records one shared conversation admission decision.".to_string(),
        tags: vec!["conversation".to_string()],
        ..Summary::default()
    };
    put_smart_content(
        &pool,
        appended.accepted[3].raw_hash(),
        SUMMARIZER,
        &conversation_summary,
    )
    .await
    .unwrap();
    put_smart_content(
        &pool,
        appended.accepted[4].raw_hash(),
        SUMMARIZER,
        &conversation_summary,
    )
    .await
    .unwrap();

    let code_smart_hash = content_hash(code_summary.text.as_bytes());
    let conversation_smart_hash = content_hash(conversation_summary.text.as_bytes());
    let embedding_rows = vec![
        (shared_raw_hash.unwrap(), SourceKind::Raw, vector(0.00)),
        (smart_a.raw_hash, SourceKind::Raw, vector(0.10)),
        (smart_b.raw_hash, SourceKind::Raw, vector(0.20)),
        (code_smart_hash, SourceKind::Smart, vector(0.03)),
        (beta_element.raw_hash, SourceKind::Raw, vector(0.01)),
        (
            appended.accepted[0].raw_hash().to_string(),
            SourceKind::Raw,
            vector(0.04),
        ),
        (
            appended.accepted[3].raw_hash().to_string(),
            SourceKind::Raw,
            vector(0.15),
        ),
        (
            appended.accepted[4].raw_hash().to_string(),
            SourceKind::Raw,
            vector(0.25),
        ),
        (conversation_smart_hash, SourceKind::Smart, vector(0.05)),
    ];
    let new_embeddings = embedding_rows
        .iter()
        .map(|(source_hash, source_kind, vector)| NewEmbedding {
            chunk_no: 0,
            source_hash,
            source_kind: *source_kind,
            vector,
            truncated: false,
        })
        .collect::<Vec<_>>();
    put_embeddings(&pool, EMBEDDER, &new_embeddings)
        .await
        .unwrap();

    let cases = vec![
        (
            "repo",
            SearchFilters {
                repo: Some(alpha.key().to_string()),
                ..SearchFilters::default()
            },
        ),
        (
            "path",
            SearchFilters {
                path: Some("src/shared/%".to_string()),
                ..SearchFilters::default()
            },
        ),
        (
            "source_raw",
            SearchFilters {
                source: Some(SourceKind::Raw),
                ..SearchFilters::default()
            },
        ),
        (
            "source_smart",
            SearchFilters {
                source: Some(SourceKind::Smart),
                ..SearchFilters::default()
            },
        ),
        (
            "kind",
            SearchFilters {
                kinds: Some(vec![ElementKind::Function]),
                ..SearchFilters::default()
            },
        ),
        (
            "conversation",
            SearchFilters {
                conversation: Some(CONVERSATION.to_string()),
                ..SearchFilters::default()
            },
        ),
    ];

    let query = vector(0.0);
    let mut actual = Map::new();
    for limit in [10, 40] {
        for (name, template) in &cases {
            let mut filters = template.clone();
            filters.limit = limit;
            let hits = search_elements(&pool, EMBEDDER, &query, &filters)
                .await
                .unwrap_or_else(|error| panic!("{name} limit {limit}: {error}"));
            actual.insert(
                format!("{name}.limit_{limit}"),
                Value::Array(
                    hits.into_iter()
                        .map(|hit| {
                            json!({
                                "address": hit.similar.element.address,
                                "score": 1.0 - hit.similar.distance,
                            })
                        })
                        .collect(),
                ),
            );
        }
    }
    let actual = Value::Object(actual);
    let expected: Value =
        serde_json::from_str(include_str!("fixtures/search_admission_golden.json"))
            .expect("search-admission golden is valid JSON");
    let comparison = compare_golden(&expected, &actual);

    database.destroy(pool).await;
    if let Err(error) = comparison {
        panic!(
            "{error}\nold-query output to capture as the golden:\n{}",
            serde_json::to_string_pretty(&actual).unwrap()
        );
    }
}
