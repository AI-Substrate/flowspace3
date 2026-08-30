//! Recovery: enrichment the content layer is missing, on both shelves.
//!
//! Until the level-0 fix in this binary, GC read every `embed` job as
//! unreferenced — an embed job carries a BATCH as `items` and has no `raw_hash`
//! field for the predicate to find — and deleted any batch still pending when a
//! pass landed. GC runs at every boot and on a cadence, so a daemon restarted
//! mid-scan with a full queue lost exactly the work it had not finished.
//!
//! The loss is invisible from every angle a person would look at: the elements
//! are there, the summaries are there, `status` reports an empty queue, and the
//! content is simply absent from every semantic search. So recovery cannot be
//! driven from the queue, which no longer remembers, or from a stored flag,
//! which nothing set. It is derived from the schema — content with no vector,
//! and elements with no summary.
//!
//! The summary half had a second cause worth stating: `missing_enrichment`, the
//! decision-D6 sweep written for exactly this, had NO production caller. It
//! existed, and only tests ever ran it.

mod support;

use fs3_core::{Config, DatabaseConfig, Element, ElementKind, RepoIdentity, Span, content_hash};
use fs3_daemon::enrich;
use fs3_daemon::wiring::AppState;
use fs3_store::PgPool;

const BODY: &str = "fn stranded() { never_embedded() }";

async fn stack(label: &str) -> (support::FreshDatabase, AppState) {
    let database = support::FreshDatabase::create(label).await;
    let config = Config {
        database: DatabaseConfig {
            url: database.url(),
        },
        ..Config::default()
    };
    let state = AppState::from_config(config).expect("the fake stack wires");
    fs3_store::migrate(&state.db).await.expect("migrates");
    (database, state)
}

/// A registered root holding one file with one declaration.
async fn live_content(state: &AppState) {
    let root = "/srv/stranded";
    let identity = RepoIdentity::from_path(std::path::Path::new(root));
    let worktree = fs3_store::register_worktree(&state.db, &identity, root, Some("main"))
        .await
        .expect("registering");
    let blob = fs3_core::BlobRef::new("b".repeat(40)).expect("a blob key");
    fs3_store::sync_worktree_files(
        &state.db,
        worktree,
        &[("src/s.rs".to_string(), blob.clone())],
    )
    .await
    .expect("mapping");

    let file = Element::new(
        ElementKind::File,
        "rust",
        "s.rs",
        "src/s.rs",
        Span::new(1, 8),
        "// src/s.rs\n",
    )
    .with_children(vec![Element::new(
        ElementKind::Function,
        "function_item",
        "stranded",
        "src/s.rs::stranded",
        Span::new(3, 5),
        BODY,
    )]);

    fs3_store::upsert_element_tree(&state.db, &blob, "test-parser@1", &file, |element| {
        element.kind != ElementKind::File
    })
    .await
    .expect("storing the parse");
}

async fn embed_jobs(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM jobs WHERE kind = 'embed'")
        .fetch_one(pool)
        .await
        .expect("counting")
}

/// The whole recovery, end to end: content whose vectors were never bought is
/// found and re-queued, and a second boot does not queue it again.
#[tokio::test]
async fn a_boot_requeues_vectors_that_were_never_bought() {
    let (database, state) = stack("recover-vectors").await;
    live_content(&state).await;

    // The damage: no embed jobs, no vectors, elements that look perfectly fine.
    assert_eq!(embed_jobs(&state.db).await, 0);

    let queued = enrich::requeue_missing_vectors(&state, 2_000)
        .await
        .expect("the sweep runs");
    assert_eq!(
        queued, 1,
        "the declaration has no vector, so it is re-queued"
    );
    assert_eq!(
        embed_jobs(&state.db).await,
        1,
        "one batch, not one job per text"
    );

    // Idempotent by content: the same sweep on the same state enqueues the same
    // batch, which the queue recognises rather than duplicating.
    enrich::requeue_missing_vectors(&state, 2_000)
        .await
        .expect("second sweep");
    assert_eq!(
        embed_jobs(&state.db).await,
        1,
        "a boot loop must not multiply the backlog"
    );

    database.destroy(state.db).await;
}

/// Once the vector exists, the sweep is silent — which is what makes running it
/// at every boot cheap rather than a queue flood.
#[tokio::test]
async fn a_healthy_index_sweeps_to_nothing() {
    let (database, state) = stack("recover-quiet").await;
    live_content(&state).await;

    let vector = fs3_core::Embedder::embed(&*state.embedder, &[BODY.to_string()])
        .await
        .expect("the fake embedder does not fail");
    fs3_store::put_embeddings(
        &state.db,
        &state.embedder.key(),
        &[fs3_store::NewEmbedding {
            chunk_no: 0,
            source_hash: &content_hash(BODY.as_bytes()),
            source_kind: fs3_store::SourceKind::Raw,
            vector: &vector[0],
            truncated: false,
        }],
    )
    .await
    .expect("writing the vector");

    assert_eq!(
        enrich::requeue_missing_vectors(&state, 2_000)
            .await
            .expect("the sweep runs"),
        0,
        "nothing is missing, so nothing is queued"
    );
    assert_eq!(embed_jobs(&state.db).await, 0);

    database.destroy(state.db).await;
}

/// And the sweep must not resurrect work for content nothing holds: a root that
/// has gone is not a backlog, it is garbage, and re-queueing it would pay a
/// provider for something no search can ever return.
#[tokio::test]
async fn a_swept_batch_for_departed_content_does_not_survive_collection() {
    let (database, state) = stack("recover-departed").await;
    live_content(&state).await;

    enrich::requeue_missing_vectors(&state, 2_000)
        .await
        .expect("the sweep runs");
    assert_eq!(embed_jobs(&state.db).await, 1);

    fs3_store::remove_root(&state.db, "/srv/stranded")
        .await
        .expect("removing");
    let reclaimed = fs3_store::collect_garbage(&state.db)
        .await
        .expect("collecting");

    assert!(
        reclaimed.jobs > 0,
        "a re-queued batch for departed content is still garbage: {reclaimed:?}"
    );
    assert_eq!(embed_jobs(&state.db).await, 0);

    database.destroy(state.db).await;
}

/// The summary shelf: an element marked for enrichment with no summary is
/// re-queued, and the job carries the ELEMENT — a summariser reads a
/// declaration's kind, name, address and span, not just its text.
#[tokio::test]
async fn a_boot_requeues_summaries_the_content_layer_is_missing() {
    let (database, state) = stack("recover-summaries").await;
    live_content(&state).await;

    let queued = enrich::requeue_missing_summaries(&state, 500)
        .await
        .expect("the sweep runs");
    assert_eq!(
        queued, 1,
        "one element is marked for enrichment and has no summary"
    );

    let payload: serde_json::Value =
        sqlx::query_scalar("SELECT payload FROM jobs WHERE kind = 'summarize'")
            .fetch_one(&state.db)
            .await
            .expect("reading the job back");

    assert_eq!(payload["raw_hash"], content_hash(BODY.as_bytes()));
    assert_eq!(
        payload["element"]["address"], "src/s.rs::stranded",
        "the job carries the real element, not an invented one"
    );
    assert_eq!(payload["element"]["kind"], "function");
    assert_eq!(payload["element"]["raw_text"], BODY);

    // Keyed by content, so a boot loop does not multiply the backlog.
    enrich::requeue_missing_summaries(&state, 500)
        .await
        .expect("second sweep");
    let jobs: i64 = sqlx::query_scalar("SELECT count(*) FROM jobs WHERE kind = 'summarize'")
        .fetch_one(&state.db)
        .await
        .expect("counting");
    assert_eq!(jobs, 1);

    database.destroy(state.db).await;
}

/// And it goes quiet once the summary lands — there is no flag to clear.
#[tokio::test]
async fn a_summarised_index_sweeps_to_nothing() {
    let (database, state) = stack("recover-summarised").await;
    live_content(&state).await;

    fs3_store::put_smart_content(
        &state.db,
        &content_hash(BODY.as_bytes()),
        &state.summarizer.key(),
        &fs3_core::Summary {
            text: "strands nothing".to_string(),
            tags: vec!["stranded".to_string()],
            extras: std::collections::BTreeMap::new(),
        },
    )
    .await
    .expect("writing the summary");

    assert_eq!(
        enrich::requeue_missing_summaries(&state, 500)
            .await
            .expect("the sweep runs"),
        0,
        "a stored summary is what makes an element clean"
    );

    database.destroy(state.db).await;
}
