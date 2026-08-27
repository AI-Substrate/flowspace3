//! Removal and garbage collection (PRD req 57), each on its own throwaway
//! database.
//!
//! The two survival cases are the reason this file exists. GC deletes paid-for
//! work, so the tests that matter are not "does it collect" — they are "does it
//! REFUSE to collect something still in use", and there are two independent
//! ways to get that wrong:
//!
//! 1. **A blob two repos hold.** Removing one root must leave the other's
//!    parse intact. This is the case the brief named.
//! 2. **A raw hash carried by elements of two DIFFERENT blobs** — the same
//!    function text in two different files, which is precisely what
//!    content-addressed enrichment exists to exploit. Collecting one blob's
//!    elements must not reap the summary the other file still needs. This one
//!    is not visible from the blob level at all, and getting it wrong violates
//!    workshop 002's decision D8 from inside the pass written to enforce it.

mod support;

use std::path::Path;

use fs3_core::{BlobRef, Element, ElementKind, RepoIdentity, Span, Summary, content_hash};
use fs3_store::{
    PgPool, collect_garbage, enqueue_job, get_smart_content, put_smart_content, reclaimable,
    register_worktree, remove_root, sync_worktree_files, upsert_element_tree, worktree_exists,
};
use std::collections::BTreeMap;
use std::time::Duration;
use support::{FreshDatabase, PARSER_VERSION, unique_blob};

const SUMMARIZER: &str = "fake-summarizer@v1";

/// A one-function file whose body is `body`, so two files can be given
/// deliberately identical or deliberately different content.
fn file_with_body(path: &str, body: &str) -> Element {
    Element::new(
        ElementKind::File,
        "rust",
        path.rsplit('/').next().unwrap_or(path),
        path,
        Span::new(1, 8),
        format!("// {path}\n"),
    )
    .with_children(vec![
        Element::new(
            ElementKind::Function,
            "function_item",
            "shared",
            format!("{path}::shared"),
            Span::new(3, 5),
            body,
        )
        .with_sibling_order(0),
    ])
}

/// Register `root_path`, map one file at `blob`, and store its parse.
async fn add_root_holding(
    pool: &PgPool,
    root_path: &str,
    file: &str,
    blob: &BlobRef,
    body: &str,
) -> i64 {
    let worktree = register_worktree(
        pool,
        &RepoIdentity::from_path(Path::new(root_path)),
        root_path,
        Some("main"),
    )
    .await
    .expect("registering the worktree");

    sync_worktree_files(pool, worktree, &[(file.to_string(), blob.clone())])
        .await
        .expect("mapping the file");

    upsert_element_tree(
        pool,
        blob,
        PARSER_VERSION,
        &file_with_body(file, body),
        |element| element.kind != ElementKind::File,
    )
    .await
    .expect("storing the parse");

    worktree
}

#[tokio::test]
async fn removing_a_root_unregisters_it_and_kills_only_its_own_queued_scans() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;

    let blob = unique_blob();
    let doomed = add_root_holding(&pool, "/srv/a", "src/a.rs", &blob, "fn a() {}").await;
    let survivor = add_root_holding(&pool, "/srv/b", "src/b.rs", &unique_blob(), "fn b() {}").await;

    // A queued scan for each root, plus enrichment that belongs to CONTENT
    // rather than to either root.
    for (worktree, path) in [(doomed, "src/a.rs"), (survivor, "src/b.rs")] {
        enqueue_job(
            &pool,
            "scan_file",
            &format!("scan:{worktree}:{path}"),
            &serde_json::json!({ "worktree_id": worktree, "path": path }),
            Duration::ZERO,
        )
        .await
        .expect("queueing a scan");
    }
    enqueue_job(
        &pool,
        "summarize",
        "summarize:github.com/x/a:deadbeef",
        &serde_json::json!({ "raw_hash": "deadbeef", "identity": "github.com/x/a" }),
        Duration::ZERO,
    )
    .await
    .expect("queueing enrichment");

    let removal = remove_root(&pool, "/srv/a").await.expect("removing");

    assert!(removal.was_registered());
    assert_eq!(removal.worktree_id, Some(doomed));
    assert_eq!(removal.files, 1);
    assert_eq!(
        removal.jobs_killed, 1,
        "exactly the removed root's own scan, and nothing else"
    );
    assert!(removal.repo_removed, "its repo had no other checkout");

    assert!(!worktree_exists(&pool, doomed).await.expect("checking"));
    assert!(
        worktree_exists(&pool, survivor).await.expect("checking"),
        "the other root must be untouched"
    );

    // D8: enrichment is keyed by content, not by root. Killing it because one
    // root left would destroy work another root may still need.
    let kinds: Vec<String> = sqlx::query_scalar("SELECT kind FROM jobs ORDER BY kind")
        .fetch_all(&pool)
        .await
        .expect("reading the queue");
    assert_eq!(
        kinds,
        vec!["scan_file".to_string(), "summarize".to_string()],
        "the survivor's scan and the content-keyed enrichment both live"
    );

    database.destroy(pool).await;
}

#[tokio::test]
async fn removing_an_unregistered_path_is_an_answer_not_a_failure() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;

    let removal = remove_root(&pool, "/srv/never-added")
        .await
        .expect("an unknown path is a question with a true answer");

    assert!(!removal.was_registered());
    assert_eq!(removal.files, 0);
    assert_eq!(removal.jobs_killed, 0);

    database.destroy(pool).await;
}

/// Survival case 1: two repos holding the SAME blob. Removing one must leave
/// the shared parse alone, because the other still maps it.
#[tokio::test]
async fn a_blob_two_repos_hold_survives_the_removal_of_one() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;

    let shared = unique_blob();
    add_root_holding(&pool, "/srv/a", "src/shared.rs", &shared, "fn shared() {}").await;
    add_root_holding(
        &pool,
        "/srv/b",
        "vendor/shared.rs",
        &shared,
        "fn shared() {}",
    )
    .await;

    remove_root(&pool, "/srv/a").await.expect("removing");
    let reclaimed = collect_garbage(&pool).await.expect("collecting");

    assert_eq!(
        reclaimed.elements, 0,
        "the surviving repo still maps that blob: {reclaimed:?}"
    );
    let remaining: i64 = sqlx::query_scalar("SELECT count(*) FROM elements WHERE blob_sha = $1")
        .bind(shared.as_str())
        .fetch_one(&pool)
        .await
        .expect("counting");
    assert!(remaining > 0, "the shared parse must still be there");

    database.destroy(pool).await;
}

/// Survival case 2, and the dangerous one: ONE raw hash carried by elements of
/// TWO DIFFERENT blobs — the same function text in two different files.
///
/// Collecting the departed blob's elements is correct. Reaping the SUMMARY of
/// that text is not: the other file still carries it, and the summary was paid
/// for. A level-two delete keyed off "the blob went away" gets this wrong,
/// which is why every level re-derives from what REMAINS.
#[tokio::test]
async fn a_summary_shared_by_another_blob_survives_collection() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;

    // Deliberately identical bodies in two different files → two blobs, one
    // raw hash.
    let body = "fn shared() { the_same_text() }";
    let raw_hash = content_hash(body.as_bytes());

    let doomed_blob = unique_blob();
    let kept_blob = unique_blob();
    add_root_holding(&pool, "/srv/a", "src/a.rs", &doomed_blob, body).await;
    add_root_holding(&pool, "/srv/b", "src/b.rs", &kept_blob, body).await;

    put_smart_content(
        &pool,
        &raw_hash,
        SUMMARIZER,
        &Summary {
            text: "does the shared thing".to_string(),
            tags: vec!["shared".to_string()],
            extras: BTreeMap::new(),
        },
    )
    .await
    .expect("storing the summary");

    remove_root(&pool, "/srv/a").await.expect("removing");
    let reclaimed = collect_garbage(&pool).await.expect("collecting");

    assert!(
        reclaimed.elements > 0,
        "the departed blob's parse SHOULD go: {reclaimed:?}"
    );
    assert_eq!(
        reclaimed.summaries, 0,
        "but its summary is still carried by the other blob's elements: {reclaimed:?}"
    );
    assert!(
        get_smart_content(&pool, &raw_hash, SUMMARIZER)
            .await
            .expect("reading back")
            .is_some(),
        "paid-for enrichment a registered root still needs must survive (D8)"
    );

    database.destroy(pool).await;
}

/// The whole chain, when nothing is shared: removing the only root that held
/// the content makes every level collectable, and a second pass finds nothing.
#[tokio::test]
async fn an_unshared_root_is_reclaimed_through_every_level_and_then_settles() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;

    let body = "fn lonely() { by_itself() }";
    let raw_hash = content_hash(body.as_bytes());
    let blob = unique_blob();
    add_root_holding(&pool, "/srv/lonely", "src/l.rs", &blob, body).await;
    put_smart_content(
        &pool,
        &raw_hash,
        SUMMARIZER,
        &Summary {
            text: "lonely".to_string(),
            tags: vec!["lonely".to_string()],
            extras: BTreeMap::new(),
        },
    )
    .await
    .expect("storing the summary");

    // Enrichment queued for content that is about to become unreferenced: it
    // would otherwise be paid for and never searchable.
    enqueue_job(
        &pool,
        "summarize",
        "summarize:github.com/x/lonely:pending",
        &serde_json::json!({ "raw_hash": raw_hash, "identity": "github.com/x/lonely" }),
        Duration::ZERO,
    )
    .await
    .expect("queueing enrichment");

    // Nothing is collectable while the root is registered.
    assert!(
        reclaimable(&pool).await.expect("counting").is_empty(),
        "a registered root's content is not garbage"
    );

    remove_root(&pool, "/srv/lonely").await.expect("removing");

    // `reclaimable` is a FLOOR, not a forecast, and this is the test that
    // pins that down. It reports what is collectable AT THIS INSTANT, level by
    // level — and the summary is not, yet, because the elements carrying its
    // raw hash are still there. They are collectABLE, not collected. Deeper
    // levels only come into view once the level above actually goes.
    //
    // Simulating the cascade read-only would mean modelling three deletes in a
    // query, which is a second implementation of the collector written in
    // SQL — so the number stays honest-and-low rather than clever-and-drifting.
    let floor = reclaimable(&pool).await.expect("counting");
    assert!(
        floor.jobs > 0,
        "the doomed enrichment is visible now: {floor:?}"
    );
    assert!(floor.elements > 0, "and so is the parse: {floor:?}");
    assert_eq!(
        floor.summaries, 0,
        "the summary is still held by elements that have not gone yet: {floor:?}"
    );

    // One pass runs the levels in order, so the cascade completes inside it.
    let reclaimed = collect_garbage(&pool).await.expect("collecting");
    assert_eq!(reclaimed.jobs, floor.jobs, "the floor must not overstate");
    assert_eq!(reclaimed.elements, floor.elements);
    assert!(
        reclaimed.summaries > 0,
        "and one pass reaches deeper than the floor could see: {reclaimed:?}"
    );

    assert!(
        get_smart_content(&pool, &raw_hash, SUMMARIZER)
            .await
            .expect("reading back")
            .is_none()
    );

    // Level-triggered: a second pass over a clean database is a no-op, which is
    // what makes running this on a cadence cheap.
    assert!(
        collect_garbage(&pool)
            .await
            .expect("second pass")
            .is_empty(),
        "GC must reach a fixed point in one pass"
    );
    assert!(reclaimable(&pool).await.expect("counting").is_empty());

    database.destroy(pool).await;
}
