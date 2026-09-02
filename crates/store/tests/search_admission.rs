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
            let hits = hits.hits;
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

fn at_distance(base: &[f32], distance: f32, seed: u64) -> Vec<f32> {
    let mut direction = vec![0.0; base.len()];
    let mut noise = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    for slot in &mut direction {
        noise ^= noise << 13;
        noise ^= noise >> 7;
        noise ^= noise << 17;
        *slot = ((noise >> 11) as f32 / (1_u64 << 53) as f32) - 0.5;
    }
    let dot: f32 = direction.iter().zip(base).map(|(d, b)| d * b).sum();
    for (slot, b) in direction.iter_mut().zip(base) {
        *slot -= dot * b;
    }
    let norm = direction.iter().map(|d| d * d).sum::<f32>().sqrt();
    for slot in &mut direction {
        *slot /= norm;
    }
    let cosine = 1.0 - distance;
    let sine = (1.0 - cosine * cosine).max(0.0).sqrt();
    base.iter()
        .zip(&direction)
        .map(|(b, d)| cosine * b + sine * d)
        .collect()
}

async fn seed_geometry(
    pool: &PgPool,
    worktree_id: i64,
    prefix: &str,
    count: i32,
    base: &[f32],
    distance: f32,
) {
    sqlx::query(
        "INSERT INTO elements
             (blob_sha, parser_version, kind, subkind, name, address,
              span_start, span_end, sibling_order, raw_text, raw_hash, enrich)
         SELECT $1 || '-blob-' || n, $2, 'function', 'function_item',
                $1 || '_' || n, 'src/' || $1 || '-' || n || '.rs::f',
                1, 1, n, $1 || ' body ' || n, $1 || '-raw-' || n, false
           FROM generate_series(1, $3) AS n",
    )
    .bind(prefix)
    .bind(PARSER_VERSION)
    .bind(count)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO worktree_files (worktree_id, path, blob_sha)
         SELECT $1, 'src/' || $2 || '-' || n || '.rs', $2 || '-blob-' || n
           FROM generate_series(1, $3) AS n",
    )
    .bind(worktree_id)
    .bind(prefix)
    .bind(count)
    .execute(pool)
    .await
    .unwrap();
    let rows = (1..=count)
        .map(|index| {
            (
                format!("{prefix}-raw-{index}"),
                at_distance(base, distance, index as u64),
            )
        })
        .collect::<Vec<_>>();
    let embeddings = rows
        .iter()
        .map(|(source_hash, vector)| NewEmbedding {
            chunk_no: 0,
            source_hash,
            source_kind: SourceKind::Raw,
            vector,
            truncated: false,
        })
        .collect::<Vec<_>>();
    put_embeddings(pool, EMBEDDER, &embeddings).await.unwrap();
}

#[tokio::test]
async fn scoped_search_passes_twelve_thousand_nearer_foreign_vectors() {
    let database = FreshDatabase::create_from(&fs3_testkit::test_database_url(), "search-paired")
        .await
        .expect("create migrated paired-geometry database");
    let pool = database.pool().await;
    let scoped = RepoIdentity::from_remote_parts(Some("github.com"), "fixtures/scoped").unwrap();
    let foreign = RepoIdentity::from_remote_parts(Some("github.com"), "fixtures/foreign").unwrap();
    let scoped_worktree = register_worktree(&pool, &scoped, "/fixtures/scoped", Some("main"))
        .await
        .unwrap();
    let foreign_worktree = register_worktree(&pool, &foreign, "/fixtures/foreign", Some("main"))
        .await
        .unwrap();
    let query = vector(0.0);
    seed_geometry(&pool, foreign_worktree, "foreign", 12_000, &query, 0.05).await;
    seed_geometry(&pool, scoped_worktree, "scoped", 5, &query, 0.10).await;
    sqlx::query("ANALYZE elements, worktree_files, embeddings_1024")
        .execute(&pool)
        .await
        .unwrap();

    let started = std::time::Instant::now();
    let page = search_elements(
        &pool,
        EMBEDDER,
        &query,
        &SearchFilters {
            repo: Some(scoped.key().to_string()),
            source: Some(SourceKind::Raw),
            limit: 10,
            ..SearchFilters::default()
        },
    )
    .await
    .expect("scoped search remains a successful short page");
    let search_elapsed = started.elapsed();
    println!(
        "paired_geometry: passes={} hits={} elapsed_ms={:.3}",
        page.passes,
        page.hits.len(),
        search_elapsed.as_secs_f64() * 1000.0
    );
    let identities = page
        .hits
        .iter()
        .map(|hit| hit.identity.as_deref())
        .collect::<Vec<_>>();

    assert_eq!(
        page.hits.len(),
        5,
        "all scoped vectors must be returned: {page:?}"
    );
    assert!(
        page.passes <= 2,
        "scope admission took {} passes",
        page.passes
    );
    assert!(!page.candidate_limit_exhausted);
    assert!(
        identities
            .iter()
            .all(|identity| *identity == Some(scoped.key())),
        "every hit must belong to the requested repository: {identities:?}"
    );
}

#[tokio::test]
async fn admitted_growth_stops_an_empty_content_filter_after_two_passes() {
    let database =
        FreshDatabase::create_from(&fs3_testkit::test_database_url(), "search-no-growth")
            .await
            .expect("create migrated no-growth database");
    let pool = database.pool().await;
    let scoped = RepoIdentity::from_remote_parts(Some("github.com"), "fixtures/no-growth").unwrap();
    let worktree = register_worktree(&pool, &scoped, "/fixtures/no-growth", Some("main"))
        .await
        .unwrap();
    let query = vector(0.0);
    seed_geometry(&pool, worktree, "function", 200, &query, 0.10).await;

    let page = search_elements(
        &pool,
        EMBEDDER,
        &query,
        &SearchFilters {
            repo: Some(scoped.key().to_string()),
            source: Some(SourceKind::Raw),
            kinds: Some(vec![ElementKind::Section]),
            limit: 10,
            ..SearchFilters::default()
        },
    )
    .await
    .expect("admission exhaustion returns an empty page, never an outage");

    database.destroy(pool).await;
    assert!(page.hits.is_empty());
    assert!(page.candidate_limit_exhausted);
    assert_eq!(
        page.passes, 2,
        "unchanged admitted count must stop pass three"
    );
}
