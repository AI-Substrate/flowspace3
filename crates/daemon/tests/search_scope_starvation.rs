//! A search narrowed to one repository must not be starved by the others.
//!
//! Jordan ran `flowspace3 search "llm"` in a fully-indexed repository and got
//! nothing back, with `--min-score 0` — a floor that makes an empty answer
//! arithmetically impossible for any query vector against a populated index.
//! The query vector was healthy (unit norm, no NaN, 1024 non-zero components)
//! and the same vector found plenty with the repository filter dropped.
//!
//! The cause was the shape of the similarity query, not the question. An HNSW
//! scan yields at most `hnsw.ef_search` candidates and the CTE's filters are
//! applied to THAT handful; a repository holding a small share of a central
//! index can therefore have every candidate deleted by its own anchor and be
//! answered with silence. On the index that produced the report, the searched
//! repository held 9.5% of the vectors and twelve ordinary questions asking for
//! ten hits each were answered with 19 of 120 — the zero was simply where the
//! silent undercount reached the floor and became visible.
//!
//! These tests build that geometry deliberately: a crowded decoy repository
//! sitting right on top of the query, and the repository actually being asked
//! about further away. Without `hnsw.iterative_scan` the scoped search returns
//! nothing; with it, it returns what it was asked for.

mod support;

use std::sync::Arc;

use fs3_core::{BlobRef, Config, DatabaseConfig, Element, ElementKind, RepoIdentity, Span};
use fs3_daemon::scope::Scope;
use fs3_daemon::search::{SearchRequest, search};
use fs3_daemon::wiring::AppState;
use fs3_store::{NewEmbedding, SearchFilters, SourceKind};
use fs3_testkit::fakes::FakeEmbedder;

/// The vector width the store holds.
const DIMS: usize = fs3_store::EMBEDDING_DIMENSIONS;

/// The question every test here asks.
const QUESTION: &str = "how does the watcher decide what to ignore";

/// The exact report table, including controls that never exhibited the empty result.
const LLM_REPRO_QUERIES: [&str; 12] = [
    "llm",
    "LLM",
    "llms",
    "lllm",
    "mll",
    "llm ",
    " llm",
    "LLM provider",
    "x",
    "gc",
    "watcher",
    "qqqzzzword provider",
];

/// How many vectors the decoy repository crowds around the question.
///
/// Only has to exceed `hnsw.ef_search`'s default of 40 by enough margin that
/// no target vector sneaks into the first batch. [`stack`] pins the plan, so
/// this does not also have to be big enough to earn it.
const DECOYS: usize = 1_000;

/// How many vectors the repository under test holds.
const TARGET_ELEMENTS: usize = 200;
const DECOY_REPO: &str = "/srv/decoy";
const TARGET_REPO: &str = "/srv/target";

/// A daemon on a throwaway database, ready to be forced onto the plan a
/// production-sized index runs.
///
/// `seq_page_cost`/`random_page_cost` say that touching a page is expensive,
/// which is the regime a large index is in and the reason a real deployment
/// reaches for the HNSW index at all. On its own it is not enough — see
/// [`pin_to_the_index_plan`], which finishes the job after the data is in.
///
/// `extra` is applied in the same breath, BEFORE the daemon's pool opens a
/// single connection. `ALTER DATABASE` takes effect at connection start, and a
/// setting applied afterwards is invisible to every connection the pool
/// already holds — a test that set it later would be reading a stale session
/// and wondering why nothing changed.
async fn stack(label: &str, extra: &[&str]) -> (support::FreshDatabase, AppState) {
    let database = support::FreshDatabase::create(label).await;

    let pinning = database.pool().await;
    for setting in [
        "seq_page_cost = 1000",
        "random_page_cost = 1000",
        "enable_seqscan = off",
    ]
    .iter()
    .chain(extra)
    {
        sqlx::query(&format!(
            "DO $$ BEGIN
                 EXECUTE format('ALTER DATABASE %I SET {setting}', current_database());
             END $$;"
        ))
        .execute(&pinning)
        .await
        .unwrap_or_else(|error| panic!("applying `{setting}`: {error}"));
    }
    pinning.close().await;

    let config = Config {
        database: DatabaseConfig {
            url: database.url(),
        },
        ..Config::default()
    };
    let mut state = AppState::from_config(config).expect("wires");
    fs3_store::migrate(&state.db).await.expect("migrates");
    state.embedder = Arc::new(FakeEmbedder {
        dimensions: DIMS,
        ..FakeEmbedder::default()
    });
    (database, state)
}

/// Embed one query through the same fake the daemon uses.
async fn query_vector(query: &str) -> Vec<f32> {
    use fs3_core::Embedder;
    FakeEmbedder {
        dimensions: DIMS,
        ..FakeEmbedder::default()
    }
    .embed(&[query.to_string()])
    .await
    .expect("the fake embeds")
    .pop()
    .expect("one vector")
}

/// The vector the daemon will embed [`QUESTION`] into.
async fn question_vector() -> Vec<f32> {
    query_vector(QUESTION).await
}

/// A unit vector at cosine distance `distance` from `base`, varied by `seed`.
///
/// Built as `cos(theta) * base + sin(theta) * orthogonal`, which puts the
/// result exactly `1 - cos(theta)` away in cosine distance. The orthogonal
/// direction is a cheap deterministic hash projected off `base`, so the points
/// at one distance form a spread rather than a single duplicated vector — an
/// HNSW graph over 4,000 copies of one point is not a graph.
fn at_distance(base: &[f32], distance: f32, seed: u64) -> Vec<f32> {
    let mut direction = vec![0f32; base.len()];
    let mut noise = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    for slot in &mut direction {
        noise ^= noise << 13;
        noise ^= noise >> 7;
        noise ^= noise << 17;
        *slot = ((noise >> 11) as f32 / (1u64 << 53) as f32) - 0.5;
    }

    // Project the base component out, so `direction` really is orthogonal and
    // the requested distance is the distance actually produced.
    let dot: f32 = direction.iter().zip(base).map(|(d, b)| d * b).sum();
    for (slot, b) in direction.iter_mut().zip(base) {
        *slot -= dot * b;
    }
    let norm: f32 = direction.iter().map(|d| d * d).sum::<f32>().sqrt();
    for slot in &mut direction {
        *slot /= norm;
    }

    let cos = 1.0 - distance;
    let sin = (1.0 - cos * cos).max(0.0).sqrt();
    base.iter()
        .zip(&direction)
        .map(|(b, d)| cos * b + sin * d)
        .collect()
}

/// Register a worktree for `root` and return its identity.
async fn repo(state: &AppState, root: &str) -> (RepoIdentity, i64) {
    let identity = RepoIdentity::from_path(std::path::Path::new(root));
    let worktree = fs3_store::register_worktree(&state.db, &identity, root, Some("main"))
        .await
        .expect("registering a worktree");
    (identity, worktree)
}

/// How many elements one fixture file holds.
///
/// Seeding goes file by file rather than element by element purely for speed:
/// this fixture needs thousands of rows before Postgres stops rewriting the
/// query into the anchor-driven plan, and a round trip per element makes that
/// a minute of waiting per test.
const ELEMENTS_PER_FILE: usize = 100;

/// Put `count` elements in `worktree`, each with a vector `distance` from the
/// question.
///
/// Real elements with real blobs and real file mappings, because the anchor the
/// bug defeats is the ref-layer join — a fixture of orphan vectors would starve
/// the scan through a different predicate and stop resembling the report.
async fn seed(
    state: &AppState,
    worktree: i64,
    prefix: &str,
    count: usize,
    distance: f32,
    base: &[f32],
) {
    let model_key = state.embedder_key("");
    let mut files = Vec::new();
    let mut vectors = Vec::with_capacity(count);
    let mut hashes = Vec::with_capacity(count);

    for (file, chunk) in (0..count)
        .collect::<Vec<_>>()
        .chunks(ELEMENTS_PER_FILE)
        .enumerate()
    {
        let path = format!("src/{prefix}_{file}.rs");
        let children: Vec<Element> = chunk
            .iter()
            .map(|index| {
                let text = format!("{prefix} element {index} of the fixture corpus");
                hashes.push(fs3_core::content_hash(text.as_bytes()));
                vectors.push(at_distance(
                    base,
                    distance,
                    *index as u64 + prefix.len() as u64,
                ));
                Element::new(
                    ElementKind::Function,
                    "function_item",
                    format!("{prefix}_{index}"),
                    format!("{path}::{prefix}_{index}"),
                    Span::new(1, 1),
                    &text,
                )
                .with_sibling_order(*index as u32)
            })
            .collect();

        let file_text = format!("{prefix} file {file}");
        let root = Element::new(
            ElementKind::File,
            "source_file",
            path.clone(),
            path.clone(),
            Span::new(1, chunk.len() as u32),
            &file_text,
        )
        .with_children(children);
        let blob = BlobRef::new(format!("{:040x}", fnv(&file_text))).expect("a blob key");

        fs3_store::upsert_element_tree(&state.db, &blob, "test-parser@1", &root, |_| false)
            .await
            .expect("storing a file's elements");

        files.push((path, blob));
    }

    fs3_store::sync_worktree_files(&state.db, worktree, &files)
        .await
        .expect("mapping files");

    let embeddings: Vec<NewEmbedding<'_>> = hashes
        .iter()
        .zip(&vectors)
        .map(|(hash, vector)| NewEmbedding {
            source_hash: hash,
            source_kind: SourceKind::Raw,
            vector,
            truncated: false,
        })
        .collect();
    fs3_store::put_embeddings(&state.db, &model_key, &embeddings)
        .await
        .expect("storing vectors");
}

/// A 40-hex blob key that differs per text without needing a hash crate.
fn fnv(text: &str) -> u128 {
    let mut hash: u128 = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d;
    for byte in text.as_bytes() {
        hash ^= u128::from(*byte);
        hash = hash.wrapping_mul(0x0000_0000_0100_0000_0000_0000_0000_013b);
    }
    hash
}

fn ask(repo: Option<&str>, min_score: Option<f64>, limit: i64) -> SearchRequest {
    SearchRequest {
        q: QUESTION.to_string(),
        repo: repo.map(str::to_string),
        limit: Some(limit),
        min_score,
        ..SearchRequest::default()
    }
}

/// The scope `--repo <identity>` resolves to. The filter the store applies
/// comes from HERE, not from the request, so a test that only set the request
/// field would search everything and prove nothing.
fn scoped(identity: &str) -> Scope {
    Scope {
        repo: Some(identity.to_string()),
        ..Scope::unscoped()
    }
}

/// Build the crowded index: decoys on top of the question, the repository
/// under test further out — then force the production plan.
async fn crowded(label: &str) -> (support::FreshDatabase, AppState, String) {
    crowded_with(label, &[]).await
}

/// [`crowded`], plus database settings applied before the pool exists.
async fn crowded_with(label: &str, extra: &[&str]) -> (support::FreshDatabase, AppState, String) {
    let (database, state) = stack(label, extra).await;
    let base = question_vector().await;

    let (_, decoy_tree) = repo(&state, DECOY_REPO).await;
    seed(&state, decoy_tree, "decoy", DECOYS, 0.05, &base).await;

    let (target_identity, target_tree) = repo(&state, TARGET_REPO).await;
    seed(&state, target_tree, "target", TARGET_ELEMENTS, 0.6, &base).await;

    pin_to_the_index_plan(&state).await;

    let identity = target_identity.to_string();
    (database, state, identity)
}

/// Leave the HNSW index as the only way to order this table.
///
/// This defect lives on exactly ONE plan, and a fixture small enough to seed in
/// a test will not choose it. Postgres has three cheaper ways to answer
/// `ORDER BY vector <=> $1 LIMIT n` over a thousand rows — sequential scan,
/// a full scan of the primary key, or inverting the query to drive from the
/// repository anchor — and every one of them is EXACT, so a fixture that lands
/// on any of them passes against the broken code. The first version of this
/// file did exactly that: three green tests, fix reverted, still green.
///
/// So the alternatives are removed rather than out-costed. `enable_seqscan` is
/// off from [`stack`]; dropping the primary key after the data is in takes the
/// other two, because every remaining path to `source_hash` and `model_key`
/// goes through a scan this database can no longer perform. What is left is
/// the HNSW index, which is what a real deployment uses and where the
/// candidate list gets eaten.
///
/// Read-only from here on, so the constraint is not needed again. `ANALYZE`
/// because a plan chosen from empty statistics is not a plan anybody chose.
async fn pin_to_the_index_plan(state: &AppState) {
    for statement in [
        "ALTER TABLE embeddings_1024 DROP CONSTRAINT embeddings_1024_pkey",
        "ANALYZE embeddings_1024",
        "ANALYZE elements",
        "ANALYZE worktree_files",
    ] {
        sqlx::query(statement)
            .execute(&state.db)
            .await
            .unwrap_or_else(|error| panic!("pinning the plan with `{statement}`: {error}"));
    }
}

/// THE regression. Ten hits asked for, ten hits returned, all from the
/// repository that was asked about — against an index where every one of the
/// nearest neighbours belongs to somebody else.
///
/// Before the fix this returns zero. Not a short list: zero, with `ok: true`,
/// which is the shape that sent a user hunting through their own code for a
/// bug that was in ours.
#[tokio::test]
async fn a_scoped_search_is_not_starved_by_a_crowded_neighbour_repository() {
    let (database, state, identity) = crowded("search-starvation-scoped").await;

    let outcome = search(
        &state,
        &ask(Some(&identity), Some(0.0), 10),
        &scoped(&identity),
    )
    .await
    .expect("a populated index answers");

    assert_eq!(
        outcome.results.len(),
        10,
        "asked for ten hits from a repository holding {}; the nearest \
         neighbours all belong to another repository, and letting them eat the candidate \
         list is the bug",
        TARGET_ELEMENTS
    );
    assert!(
        outcome.results.iter().all(|hit| hit.path.is_some()
            && hit
                .path
                .as_deref()
                .expect("a live path")
                .starts_with("src/target_")),
        "every hit must come from the repository that was asked about: {:?}",
        outcome.results.iter().map(|h| &h.path).collect::<Vec<_>>()
    );
    assert!(
        outcome.empty_because.is_none(),
        "an answered search carries no explanation for being empty"
    );

    database.destroy(state.db).await;
}

/// The original `llm` report table through the SEMANTIC leg alone.
///
/// Each query gets its own crowded pair so its geometry is deterministic: 64
/// foreign vectors are nearer than the 10 scoped vectors. Calling
/// [`fs3_store::search_elements`] directly keeps #74's lexical channel entirely
/// out of the proof; lexical text cannot rescue a starved ANN scan here.
#[tokio::test]
async fn llm_repro_queries_return_scoped_hits_without_the_lexical_leg() {
    let (database, state) = stack("search-starvation-llm-table", &[]).await;
    let mut cases = Vec::with_capacity(LLM_REPRO_QUERIES.len());

    for (case, query) in LLM_REPRO_QUERIES.iter().enumerate() {
        let vector = query_vector(query.trim()).await;
        let decoy_root = format!("{DECOY_REPO}/{case}");
        let target_root = format!("{TARGET_REPO}/{case}");
        let (_, decoy_tree) = repo(&state, &decoy_root).await;
        let (target_identity, target_tree) = repo(&state, &target_root).await;
        seed(
            &state,
            decoy_tree,
            &format!("llm_decoy_{case}"),
            64,
            0.05,
            &vector,
        )
        .await;
        seed(
            &state,
            target_tree,
            &format!("llm_target_{case}"),
            10,
            0.6,
            &vector,
        )
        .await;
        cases.push((*query, target_identity.to_string(), vector));
    }

    pin_to_the_index_plan(&state).await;
    let model_key = state.embedder_key("");
    for (query, identity, vector) in cases {
        let hits = fs3_store::search_elements(
            &state.db,
            &model_key,
            &vector,
            &SearchFilters {
                repo: Some(identity.clone()),
                source: Some(SourceKind::Raw),
                kinds: Some(vec![ElementKind::Function]),
                max_distance: Some(1.0),
                limit: 10,
                ..SearchFilters::default()
            },
        )
        .await
        .unwrap_or_else(|error| panic!("semantic search for {query:?}: {error}"));

        assert_eq!(
            hits.len(),
            10,
            "semantic-only search for {query:?} must fill the scoped limit"
        );
        assert!(
            hits.iter()
                .all(|hit| hit.identity.as_deref() == Some(&identity)
                    && hit.similar.source_kind == SourceKind::Raw),
            "semantic-only search for {query:?} leaked or changed vector spaces: {hits:?}"
        );
    }

    database.destroy(state.db).await;
}

/// The undercount, which is the same defect one step before it becomes visible.
///
/// A scoped search that returns SOME hits looks like a working search, so this
/// is the case nobody would ever have reported: the answer is simply worse than
/// the index can support, silently and forever.
#[tokio::test]
async fn a_scoped_search_returns_the_whole_limit_not_whatever_survived() {
    let (database, state, identity) = crowded("search-starvation-undercount").await;

    for limit in [1, 5, 25, 40] {
        let outcome = search(
            &state,
            &ask(Some(&identity), Some(0.0), limit),
            &scoped(&identity),
        )
        .await
        .expect("a populated index answers");

        assert_eq!(
            outcome.results.len(),
            limit as usize,
            "asked for {limit} of {} available and got {}",
            TARGET_ELEMENTS,
            outcome.results.len()
        );
    }

    database.destroy(state.db).await;
}

/// The unscoped control. The decoys ARE the nearest neighbours, so an unscoped
/// search was never broken — which is why the bug hid for so long, and why this
/// test has to prove the fix did not simply widen everything into mush.
#[tokio::test]
async fn an_unscoped_search_still_ranks_the_nearest_first() {
    let (database, state, _) = crowded("search-starvation-unscoped").await;

    let outcome = search(&state, &ask(None, Some(0.0), 10), &Scope::unscoped())
        .await
        .expect("a populated index answers");

    assert_eq!(outcome.results.len(), 10);
    assert!(
        outcome.results.iter().all(|hit| hit
            .path
            .as_deref()
            .is_some_and(|p| p.starts_with("src/decoy_"))),
        "the decoys sit at distance 0.05 and the target at 0.6, so unfiltered ranking must \
         still prefer the decoys: {:?}",
        outcome.results.iter().map(|h| &h.path).collect::<Vec<_>>()
    );

    database.destroy(state.db).await;
}

/// A floor that nothing clears is a fact, and the envelope says which floor.
///
/// The targets sit at cosine distance 0.6, so they score 0.4 — well under this
/// floor. That IS an empty answer, and the honest report names the number the
/// caller chose rather than sending them off to check `doctor`.
#[tokio::test]
async fn an_empty_answer_under_a_floor_names_the_floor() {
    let (database, state, identity) = crowded("search-starvation-floor").await;

    let outcome = search(
        &state,
        &ask(Some(&identity), Some(0.99), 10),
        &scoped(&identity),
    )
    .await
    .expect("a populated index answers");

    assert!(outcome.results.is_empty(), "nothing scores 0.99 here");
    let reason = outcome
        .empty_because
        .expect("an empty answer under a floor knows why it is empty");
    assert_eq!(reason.reason, "below_floor");
    assert!(
        reason.detail.contains("0.990"),
        "the floor the caller chose is the fact worth reporting: {}",
        reason.detail
    );

    database.destroy(state.db).await;
}

/// A glob that cannot match the indexed layout is not evidence of code absence.
#[tokio::test]
async fn an_unmatched_path_filter_names_the_layout_on_the_wire() {
    let (database, state) = stack("search-path-unmatched", &[]).await;
    let base_vector = question_vector().await;
    let (identity, worktree) = repo(&state, TARGET_REPO).await;
    seed(&state, worktree, "target", 1, 0.1, &base_vector).await;
    let identity = identity.to_string();
    let mut request = ask(Some(&identity), None, 10);
    request.path = Some("apps/**".to_string());

    let outcome = search(&state, &request, &scoped(&identity))
        .await
        .expect("an unmatched path is an explained empty answer");
    let reason = outcome
        .empty_because
        .expect("the path filter is known to match zero indexed paths");
    assert_eq!(reason.reason, "path_unmatched");
    assert!(reason.detail.contains("apps/**"), "{}", reason.detail);
    assert!(
        reason
            .hint
            .as_deref()
            .is_some_and(|hint| hint.contains("src")),
        "the correction names the indexed layout: {:?}",
        reason.hint
    );

    let auth = support::auth("search-path-unmatched");
    let base = support::spawn(fs3_daemon::http::router(state.clone(), auth.auth)).await;
    let envelope: serde_json::Value = reqwest::Client::new()
        .get(format!("{base}/search"))
        .bearer_auth(&auth.key)
        .query(&[
            ("q", QUESTION),
            ("repo", identity.as_str()),
            ("path", "apps/**"),
        ])
        .send()
        .await
        .expect("the daemon answers")
        .json()
        .await
        .expect("an envelope");

    assert_eq!(
        envelope["meta"]["empty_because"]["reason"],
        "path_unmatched"
    );
    assert!(
        envelope["meta"]["empty_because"]["hint"]
            .as_str()
            .is_some_and(|hint| hint.contains("src"))
    );
    assert!(
        envelope["next_action"]
            .as_str()
            .is_some_and(|next| next.contains("src"))
    );

    database.destroy(state.db).await;
}
/// A repository with nothing indexed in it must be told so, as an ERROR.
///
/// This is the cause nobody guesses: the index is full, the model is right, and
/// the one repository being asked about was never added. `ok: true, results: []`
/// sends the user to rephrase a question that was never the problem.
#[tokio::test]
async fn a_repository_with_no_index_is_named_rather_than_answered_with_silence() {
    let (database, state, _) = crowded("search-starvation-noindex").await;

    let elsewhere = "path:/srv/never-added";
    let failure = search(
        &state,
        &ask(Some(elsewhere), Some(0.0), 10),
        &scoped(elsewhere),
    )
    .await
    .expect_err("a repository with nothing indexed is an error, not an empty answer");

    assert_eq!(failure.code, "FS3-E-QUERY-NO-INDEX");
    assert!(
        failure.message.contains(elsewhere),
        "the anchor that came up empty has to be NAMED: {}",
        failure.message
    );
    assert!(
        failure.fix.contains("flowspace3 add"),
        "and the fix is to index it: {}",
        failure.fix
    );

    database.destroy(state.db).await;
}

/// The explanation has to reach the WIRE, not just the function.
///
/// `meta` is where a fact about the answer belongs (workshop 004), and an agent
/// reading the envelope is the consumer this was written for — so the contract
/// under test is the JSON, not the Rust type. Driven over the real router for
/// that reason.
#[tokio::test]
async fn the_envelope_carries_the_explanation_for_an_empty_answer() {
    let (database, state, identity) = crowded("search-starvation-envelope").await;

    let auth = support::auth("search-starvation-envelope");
    let base = support::spawn(fs3_daemon::http::router(state.clone(), auth.auth)).await;
    let envelope: serde_json::Value = reqwest::Client::new()
        .get(format!("{base}/search"))
        .bearer_auth(&auth.key)
        .query(&[
            ("q", QUESTION),
            ("repo", identity.as_str()),
            ("min_score", "0.99"),
        ])
        .send()
        .await
        .expect("the daemon answers")
        .json()
        .await
        .expect("an envelope");

    assert_eq!(
        envelope["ok"], true,
        "an explained empty is still an answer"
    );
    assert_eq!(
        envelope["data"]["results"].as_array().map(Vec::len),
        Some(0)
    );
    assert_eq!(envelope["meta"]["empty_because"]["reason"], "below_floor");

    // And the steer stops guessing: with a known reason in hand, repeating the
    // suspect list would be noise next to a fact.
    let next = envelope["next_action"]
        .as_str()
        .expect("every envelope steers");
    assert!(
        next.contains("0.990"),
        "the steer says the known thing: {next}"
    );
    assert!(
        !next.contains("run `flowspace3 doctor`"),
        "and stops listing suspects it has already ruled out: {next}"
    );

    database.destroy(state.db).await;
}
