//! The ref layer, the admin surface, and the filtered search — the flows plan
//! 003 added, each on its own throwaway database.
//!
//! Throwaway rather than a unique key in the shared database for the same
//! reason `pg_store_flows.rs` says: `search_elements` ranks over every vector
//! in the table and `list_worktrees` returns every row, so a concurrent test's
//! data would not merely coexist — it would be a candidate answer.

mod support;

use std::path::Path;
use std::time::Duration;

use fs3_core::{BlobRef, Element, ElementKind, Embedder, RepoIdentity, Span, Summary};
use fs3_store::{
    EMBEDDING_DIMENSIONS, JOB_PRIORITY_NEW_WORKTREE_SCAN, NewEmbedding, PgPool, SearchFilters,
    SourceKind, StoreError, claim_job, complete_job, create_database, database_exists, enqueue_job,
    enqueue_job_with_priority, find_worktree, list_worktrees, maintenance_url, put_embeddings,
    put_smart_content, queue_depth, register_worktree, retry_job, schema_current, search_elements,
    sync_worktree_files, upsert_element_tree, worktree_paths_for_blob,
};
use fs3_testkit::fakes::FakeEmbedder;
use support::{FreshDatabase, PARSER_VERSION, unique_blob, unique_seed};

const EMBEDDER: &str = "fake-embedder@v1";
const SUMMARIZER: &str = "fake-summarizer@v1";

/// A file element holding one function per body.
fn file_with(path: &str, bodies: &[&str]) -> Element {
    let children = bodies
        .iter()
        .enumerate()
        .map(|(index, body)| {
            let line = index as u32 * 4 + 3;
            Element::new(
                ElementKind::Function,
                "function_item",
                format!("f{index}"),
                format!("{path}::f{index}"),
                Span::new(line, line + 2),
                *body,
            )
            .with_sibling_order(index as u32)
        })
        .collect();

    Element::new(
        ElementKind::File,
        "rust",
        path,
        path,
        Span::new(1, 40),
        bodies.join("\n"),
    )
    .with_children(children)
}

/// Embed `text` with the fake at the store's real width, so the vector is
/// something `embeddings_1024` will actually hold.
async fn vector_for(text: &str) -> Vec<f32> {
    let embedder = FakeEmbedder {
        dimensions: EMBEDDING_DIMENSIONS,
        ..FakeEmbedder::default()
    };
    embedder
        .embed(&[text.to_string()])
        .await
        .expect("the fake never fails unless told to")
        .remove(0)
}

/// Index one file's bodies as raw vectors, and register them at `path` in
/// `worktree_id`. Returns the blob the content was written under.
async fn index_file(pool: &PgPool, worktree_id: i64, path: &str, bodies: &[&str]) -> BlobRef {
    let blob = unique_blob();
    let root = file_with(path, bodies);
    upsert_element_tree(pool, &blob, PARSER_VERSION, &root, |element| {
        element.kind != ElementKind::File
    })
    .await
    .expect("the tree should write");

    let mut vectors = Vec::new();
    for element in root.iter() {
        vectors.push((
            element.raw_hash().to_string(),
            vector_for(&element.raw_text).await,
        ));
    }
    let rows: Vec<NewEmbedding<'_>> = vectors
        .iter()
        .map(|(hash, vector)| NewEmbedding {
            chunk_no: 0,
            source_hash: hash,
            source_kind: SourceKind::Raw,
            vector,
            truncated: false,
        })
        .collect();
    put_embeddings(pool, EMBEDDER, &rows)
        .await
        .expect("vectors should write");

    sync_worktree_files(pool, worktree_id, &[(path.to_string(), blob.clone())])
        .await
        .expect("the path map should write");
    blob
}

fn identity_of(root: &str) -> RepoIdentity {
    RepoIdentity::from_path(Path::new(root))
}

// ---------------------------------------------------------------------------
// Ref layer

/// Registering the same root twice is a re-scan request, not a duplicate — and
/// a second repo row for one identity would fork derived content that the whole
/// blob-keyed design exists to share.
#[tokio::test]
async fn registering_a_root_twice_returns_the_same_worktree() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;

    let identity = identity_of("/srv/code/api");
    let first = register_worktree(&pool, &identity, "/srv/code/api", Some("main"))
        .await
        .unwrap();
    let second = register_worktree(&pool, &identity, "/srv/code/api", Some("feature"))
        .await
        .unwrap();

    assert_eq!(first, second, "one root is one worktree row");
    let worktrees = list_worktrees(&pool).await.unwrap();
    assert_eq!(worktrees.len(), 1);
    assert_eq!(
        worktrees[0].ref_name.as_deref(),
        Some("feature"),
        "a re-add refreshes the ref name rather than ignoring it"
    );

    database.destroy(pool).await;
}

/// Two checkouts of ONE repository share the repo row: that sharing is what
/// makes forty branches cost one parse (workshop 002, decision D2).
#[tokio::test]
async fn two_worktrees_of_one_repository_share_its_identity() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;

    let identity = RepoIdentity::from_remote_parts(Some("github.com"), "AI-Substrate/flowspace3")
        .expect("a host and a path make an identity");
    let main = register_worktree(&pool, &identity, "/srv/code/fs3", Some("main"))
        .await
        .unwrap();
    let feature = register_worktree(&pool, &identity, "/srv/code/fs3-feature", Some("wip"))
        .await
        .unwrap();

    assert_ne!(main, feature, "two roots are two worktrees");
    let worktrees = list_worktrees(&pool).await.unwrap();
    assert_eq!(worktrees.len(), 2);
    assert!(
        worktrees
            .iter()
            .all(|w| w.identity == "git:github.com/AI-Substrate/flowspace3"),
        "both checkouts key to one repository"
    );

    database.destroy(pool).await;
}

/// The file map is replaced, not merged: a file deleted from disk must stop
/// being findable immediately, and the derived content it pointed at must
/// survive (decision D8 — a worktree change never destroys paid-for work).
#[tokio::test]
async fn syncing_a_worktree_forgets_paths_that_are_gone_and_keeps_their_content() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;

    let identity = identity_of("/srv/code/api");
    let worktree = register_worktree(&pool, &identity, "/srv/code/api", None)
        .await
        .unwrap();

    let kept = unique_blob();
    let doomed = unique_blob();
    sync_worktree_files(
        &pool,
        worktree,
        &[
            ("src/kept.rs".to_string(), kept.clone()),
            ("src/doomed.rs".to_string(), doomed.clone()),
        ],
    )
    .await
    .unwrap();

    // The content layer holds a row for the doomed blob; deleting the pointer
    // must not touch it.
    upsert_element_tree(
        &pool,
        &doomed,
        PARSER_VERSION,
        &file_with("src/doomed.rs", &["fn gone() {}"]),
        |_| false,
    )
    .await
    .unwrap();

    let removed = sync_worktree_files(
        &pool,
        worktree,
        &[("src/kept.rs".to_string(), kept.clone())],
    )
    .await
    .unwrap();

    assert_eq!(removed, 1, "exactly the vanished path is forgotten");
    assert!(
        worktree_paths_for_blob(&pool, doomed.as_str())
            .await
            .unwrap()
            .is_empty(),
        "the deleted file has no live path"
    );
    assert!(
        fs3_store::get_elements(&pool, &doomed, PARSER_VERSION)
            .await
            .unwrap()
            .tree
            .is_some(),
        "its parsed content survives the pointer — D8 refuses the cascade"
    );

    database.destroy(pool).await;
}

/// The reverse lookup the `worktree_files_blob_sha_idx` index exists for: one
/// blob, every live path holding it. This is how a content hit becomes an
/// answer somebody can open.
#[tokio::test]
async fn one_blob_resolves_to_every_live_path_that_holds_it() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;

    let identity =
        RepoIdentity::from_remote_parts(Some("github.com"), "AI-Substrate/flowspace3").unwrap();
    let main = register_worktree(&pool, &identity, "/srv/fs3", None)
        .await
        .unwrap();
    let feature = register_worktree(&pool, &identity, "/srv/fs3-wip", None)
        .await
        .unwrap();

    let shared = unique_blob();
    sync_worktree_files(&pool, main, &[("src/lib.rs".to_string(), shared.clone())])
        .await
        .unwrap();
    sync_worktree_files(
        &pool,
        feature,
        &[("crates/core/src/lib.rs".to_string(), shared.clone())],
    )
    .await
    .unwrap();

    let paths = worktree_paths_for_blob(&pool, shared.as_str())
        .await
        .unwrap();
    assert_eq!(paths.len(), 2, "the same bytes live at two paths");
    assert_eq!(paths[0].root_path, "/srv/fs3");
    assert_eq!(paths[0].path, "src/lib.rs");
    assert_eq!(paths[1].path, "crates/core/src/lib.rs");
    assert!(paths.iter().all(|p| p.identity == identity.key()));

    database.destroy(pool).await;
}

/// `flowspace3 scan <path>` needs to tell "never added" from "nothing changed";
/// a silent no-op for an unregistered root is a bug that looks like success.
#[tokio::test]
async fn an_unregistered_root_is_findable_as_absent() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;

    assert!(
        find_worktree(&pool, "/srv/never-added")
            .await
            .unwrap()
            .is_none()
    );
    register_worktree(&pool, &identity_of("/srv/added"), "/srv/added", None)
        .await
        .unwrap();
    assert!(find_worktree(&pool, "/srv/added").await.unwrap().is_some());

    database.destroy(pool).await;
}

// ---------------------------------------------------------------------------
// Queue: retry policy support

/// The store gains a verb, not a schedule. What this proves is the verb's
/// contract: the row comes back as `pending`, invisible until its delay
/// elapses, with `attempts` untouched and the error kept.
#[tokio::test]
async fn a_retried_job_returns_to_the_queue_invisible_until_its_delay_elapses() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;

    enqueue_job(
        &pool,
        "scan_file",
        "scan:1:src/foo.rs",
        &serde_json::json!({ "path": "src/foo.rs" }),
        Duration::ZERO,
    )
    .await
    .unwrap();

    let claimed = claim_job(&pool, &["scan_file"]).await.unwrap().unwrap();
    assert_eq!(claimed.attempts, 1);

    retry_job(
        &pool,
        claimed.id,
        Duration::from_secs(60),
        "provider timeout",
    )
    .await
    .unwrap();

    assert!(
        claim_job(&pool, &["scan_file"]).await.unwrap().is_none(),
        "a backed-off job is not ready yet — that is the whole point of the delay"
    );

    // Due again: the same row, its attempt count carried forward rather than
    // reset, so a worker can tell a third attempt from a first.
    retry_job(&pool, claimed.id, Duration::ZERO, "provider timeout")
        .await
        .unwrap();
    let again = claim_job(&pool, &["scan_file"]).await.unwrap().unwrap();
    assert_eq!(again.id, claimed.id);
    assert_eq!(again.attempts, 2, "claim increments; retry must not reset");

    database.destroy(pool).await;
}

/// Equal-priority work is LIFO: a fresh edit must not wait behind a bootstrap
/// backlog that happened to arrive first.
#[tokio::test]
async fn a_fresh_job_claims_ahead_of_an_old_equal_priority_backlog() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;

    for index in 0..3 {
        enqueue_job(
            &pool,
            "scan_file",
            &format!("scan:old:{index}"),
            &serde_json::json!({ "generation": "old" }),
            Duration::ZERO,
        )
        .await
        .unwrap();
    }
    enqueue_job(
        &pool,
        "scan_file",
        "scan:fresh",
        &serde_json::json!({ "generation": "fresh" }),
        Duration::ZERO,
    )
    .await
    .unwrap();

    let claimed = claim_job(&pool, &["scan_file"]).await.unwrap().unwrap();
    assert_eq!(claimed.dedupe_key, "scan:fresh");

    complete_job(&pool, claimed.id).await.unwrap();
    enqueue_job(
        &pool,
        "scan_file",
        "scan:future",
        &serde_json::json!({ "generation": "future" }),
        Duration::from_secs(60),
    )
    .await
    .unwrap();
    let ready = claim_job(&pool, &["scan_file"]).await.unwrap().unwrap();
    assert!(
        ready.dedupe_key.starts_with("scan:old:"),
        "a newer id whose not_before is in the future must remain deferred"
    );

    database.destroy(pool).await;
}

/// Priority is global across eligible kinds. A kind-leading access path may
/// narrow a lane, but it must never turn the caller's kind order or a newer id
/// into a stronger signal than the queue's declared priority.
#[tokio::test]
async fn priority_beats_recency_across_job_kinds() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;

    enqueue_job_with_priority(
        &pool,
        "scan_file",
        "scan:promoted",
        &serde_json::json!({}),
        Duration::ZERO,
        JOB_PRIORITY_NEW_WORKTREE_SCAN,
    )
    .await
    .unwrap();
    enqueue_job(
        &pool,
        "summarize",
        "summarize:newer-default",
        &serde_json::json!({}),
        Duration::ZERO,
    )
    .await
    .unwrap();

    let claimed = claim_job(&pool, &["summarize", "scan_file"])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        claimed.dedupe_key, "scan:promoted",
        "priority must win across kinds before kind-list order or newer id"
    );

    database.destroy(pool).await;
}

/// Status reports what is *left*, per kind — "142 pending embed, 0 pending
/// scan_file" says the scan finished; one total says nothing.
#[tokio::test]
async fn queue_depth_is_grouped_by_kind_and_state() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;

    for index in 0..3 {
        enqueue_job(
            &pool,
            "scan_file",
            &format!("scan:1:f{index}.rs"),
            &serde_json::json!({}),
            Duration::ZERO,
        )
        .await
        .unwrap();
    }
    enqueue_job(
        &pool,
        "embed",
        "embed:abc",
        &serde_json::json!({}),
        Duration::ZERO,
    )
    .await
    .unwrap();
    let claimed = claim_job(&pool, &["scan_file"]).await.unwrap().unwrap();
    fs3_store::complete_job(&pool, claimed.id).await.unwrap();

    let depth = queue_depth(&pool).await.unwrap();
    let find = |kind: &str, state: &str| {
        depth
            .iter()
            .find(|row| row.kind == kind && row.state == state)
            .map_or(0, |row| row.depth)
    };
    assert_eq!(find("scan_file", "pending"), 2);
    assert_eq!(find("scan_file", "done"), 1);
    assert_eq!(find("embed", "pending"), 1);
    assert_eq!(
        find("embed", "running"),
        0,
        "a kind that has never run is absent, not a zero row"
    );

    database.destroy(pool).await;
}

/// The boot sweep. A row left `running` has no lease and no heartbeat, so
/// nothing else can ever move it — and it keeps occupying the live-dedupe
/// index, which is what turns one dead worker into a file that can never be
/// scanned again.
///
/// Sound only at boot, and only because fs3 has a single writer: at that
/// instant no worker exists to be holding a claim.
#[tokio::test]
async fn the_boot_sweep_frees_rows_a_dead_worker_was_holding() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;

    for path in ["src/wedged.rs", "src/waiting.rs"] {
        enqueue_job(
            &pool,
            "scan_file",
            &format!("scan:1:{path}"),
            &serde_json::json!({}),
            Duration::ZERO,
        )
        .await
        .unwrap();
    }

    // Claim one and abandon it — the corpse a killed worker leaves behind.
    let abandoned = claim_job(&pool, &["scan_file"]).await.unwrap().unwrap();

    let swept = fs3_store::requeue_running(&pool).await.unwrap();
    assert_eq!(swept, 1, "exactly the running row is swept");

    // Both are claimable again, and the swept one KEPT its attempt count, so a
    // job that keeps killing its worker is visible as such rather than looping
    // forever at attempt one.
    let mut seen = Vec::new();
    while let Some(job) = claim_job(&pool, &["scan_file"]).await.unwrap() {
        seen.push((job.id, job.attempts));
    }
    assert_eq!(seen.len(), 2, "nothing is left wedged");
    let recovered = seen
        .iter()
        .find(|(id, _)| *id == abandoned.id)
        .expect("the abandoned row came back");
    assert_eq!(
        recovered.1, 2,
        "the sweep does not reset attempts — claim_job already counted the first one"
    );

    // A sweep with nothing to do is a no-op, which is what makes running it on
    // every boot free.
    for (id, _) in &seen {
        fs3_store::complete_job(&pool, *id).await.unwrap();
    }
    assert_eq!(fs3_store::requeue_running(&pool).await.unwrap(), 0);

    database.destroy(pool).await;
}

// ---------------------------------------------------------------------------
// Admin

/// A fresh database is unmigrated, not broken: the check must say "everything
/// is missing" rather than fail on the absent `_sqlx_migrations` table, or the
/// first run of a new stack reports an outage.
#[tokio::test]
async fn schema_status_reads_an_unmigrated_database_as_behind_not_broken() {
    let database = FreshDatabase::create().await;
    let pool = database.pool().await;

    let before = schema_current(&pool).await.unwrap();
    assert!(!before.is_current(), "nothing has been applied yet");
    assert_eq!(before.applied, Vec::<i64>::new());
    assert_eq!(before.missing, before.embedded);
    assert!(!before.embedded.is_empty(), "the binary carries migrations");

    fs3_store::migrate(&pool).await.unwrap();

    let after = schema_current(&pool).await.unwrap();
    assert!(after.is_current(), "after migrating, nothing is missing");
    assert_eq!(after.applied, after.embedded);
    assert!(after.ahead().is_empty());

    database.destroy(pool).await;
}

/// Doctor's second step, proven end to end: absent → create → present. This is
/// the mechanism behind dw-0109's no-database path.
#[tokio::test]
async fn a_missing_database_can_be_detected_and_created() {
    let admin = fs3_store::connect(&support::database_url())
        .await
        .expect("the shared stack must be up for the store suite");
    let name = format!("fs3_doctor_{:032x}", unique_seed());

    assert!(
        !database_exists(&admin, &name).await.unwrap(),
        "a name nobody has used must not exist"
    );
    create_database(&admin, &name).await.unwrap();
    assert!(
        database_exists(&admin, &name).await.unwrap(),
        "after creating it, it exists — this is doctor's repair step"
    );

    sqlx_drop(&admin, &name).await;
    admin.close().await;
}

/// `CREATE DATABASE` cannot take a bind parameter, so this validation is the
/// only thing between a config URL and an interpolated statement.
#[tokio::test]
async fn creating_a_database_refuses_a_name_that_would_need_escaping() {
    let admin = fs3_store::connect(&support::database_url()).await.unwrap();

    let error = create_database(&admin, "fs3\"; DROP SCHEMA public CASCADE; --")
        .await
        .expect_err("an unquotable name must never reach a statement");
    assert!(matches!(error, StoreError::InvalidName(_)), "got {error:?}");

    admin.close().await;
}

/// Doctor connects to the maintenance database in order to ask about the one
/// that is missing; a URL that survives the split is what makes that possible.
#[tokio::test]
async fn the_maintenance_url_reaches_the_same_server() {
    let (maintenance, name) = maintenance_url(&support::database_url()).unwrap();
    let admin = fs3_store::connect(&maintenance)
        .await
        .expect("the maintenance database exists on every Postgres server");
    assert!(
        database_exists(&admin, &name).await.unwrap(),
        "the configured database is visible from the maintenance leg"
    );
    admin.close().await;
}

async fn sqlx_drop(admin: &PgPool, name: &str) {
    fs3_store::drop_database(admin, name)
        .await
        .unwrap_or_else(|error| panic!("dropping {name}: {error}"));
}

// ---------------------------------------------------------------------------
// Filtered search

/// The claim workshop 003 makes: filters narrow candidates IN SQL. Two repos
/// hold text about the same subject; `--repo` must exclude the other one's
/// vectors from the ranking, not from the printout.
#[tokio::test]
async fn a_repo_filter_excludes_the_other_repository_from_the_ranking() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;

    let api = identity_of("/srv/api");
    let web = identity_of("/srv/web");
    let api_tree = register_worktree(&pool, &api, "/srv/api", None)
        .await
        .unwrap();
    let web_tree = register_worktree(&pool, &web, "/srv/web", None)
        .await
        .unwrap();

    index_file(
        &pool,
        api_tree,
        "src/auth.rs",
        &["fn validate_session_token(token: &str) -> bool { true }"],
    )
    .await;
    index_file(
        &pool,
        web_tree,
        "src/auth.rs",
        &["fn validate_session_token(token: &str) -> bool { false }"],
    )
    .await;

    let query = vector_for("validate session token").await;

    let everywhere = search_elements(&pool, EMBEDDER, &query, &SearchFilters::default())
        .await
        .unwrap()
        .hits;
    assert!(
        everywhere.len() >= 2,
        "unfiltered, both repositories compete"
    );

    let scoped = search_elements(
        &pool,
        EMBEDDER,
        &query,
        &SearchFilters {
            repo: Some(api.key().to_string()),
            ..SearchFilters::default()
        },
    )
    .await
    .unwrap()
    .hits;
    assert!(!scoped.is_empty(), "the filter must not empty the result");
    assert!(
        scoped
            .iter()
            .all(|hit| hit.identity.as_deref() == Some(api.key())),
        "every hit belongs to the requested repository"
    );

    database.destroy(pool).await;
}

/// One repository may have several live checkouts with different bytes at the
/// same path. The caller's root must constrain candidates before ranking: a
/// post-LIMIT filter can both under-fill the page and leak a foreign version.
#[tokio::test]
async fn a_worktree_filter_excludes_versions_the_caller_cannot_open() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;

    let identity =
        RepoIdentity::from_remote_parts(Some("github.com"), "AI-Substrate/flowspace3").unwrap();
    let main_root = "/srv/fs3";
    let feature_root = "/srv/fs3-feature";
    let main = register_worktree(&pool, &identity, main_root, Some("main"))
        .await
        .unwrap();
    let feature = register_worktree(&pool, &identity, feature_root, Some("feature"))
        .await
        .unwrap();

    let main_version = index_file(
        &pool,
        main,
        "src/version.rs",
        &["fn checkout_version() { println!(\"main stable\") }"],
    )
    .await;
    let feature_version = index_file(
        &pool,
        feature,
        "src/version.rs",
        &["fn checkout_version() { println!(\"feature experimental marker\") }"],
    )
    .await;
    let feature_only = index_file(
        &pool,
        feature,
        "src/feature_only.rs",
        &["fn feature_only_marker() { println!(\"feature experimental marker\") }"],
    )
    .await;
    let shared = index_file(
        &pool,
        main,
        "src/shared.rs",
        &["fn shared_between_checkouts() {}"],
    )
    .await;

    sync_worktree_files(
        &pool,
        main,
        &[
            ("src/version.rs".to_string(), main_version),
            ("src/shared.rs".to_string(), shared.clone()),
        ],
    )
    .await
    .unwrap();
    sync_worktree_files(
        &pool,
        feature,
        &[
            ("src/version.rs".to_string(), feature_version),
            ("src/feature_only.rs".to_string(), feature_only),
            ("src/shared.rs".to_string(), shared),
        ],
    )
    .await
    .unwrap();

    let query = vector_for("feature experimental marker").await;
    let unscoped = search_elements(&pool, EMBEDDER, &query, &SearchFilters::default())
        .await
        .unwrap()
        .hits;
    assert!(
        unscoped
            .iter()
            .any(|hit| hit.root_path.as_deref() == Some(feature_root)),
        "without a caller root, feature content remains searchable"
    );

    let from_main = search_elements(
        &pool,
        EMBEDDER,
        &query,
        &SearchFilters {
            repo: Some(identity.key().to_string()),
            worktree: Some(main_root.to_string()),
            ..SearchFilters::default()
        },
    )
    .await
    .unwrap()
    .hits;

    assert!(
        !from_main.is_empty(),
        "the caller's own version still ranks"
    );
    assert!(
        from_main
            .iter()
            .all(|hit| hit.root_path.as_deref() == Some(main_root)),
        "every result names and belongs to the caller checkout"
    );
    assert!(
        from_main
            .iter()
            .all(|hit| hit.path.as_deref() != Some("src/feature_only.rs")),
        "a path absent from the caller checkout is excluded"
    );
    let version = from_main
        .iter()
        .find(|hit| hit.path.as_deref() == Some("src/version.rs"))
        .expect("the caller's version of the divergent path remains");
    assert!(
        version.similar.element.raw_text.contains("main stable"),
        "the same path resolves to the caller's bytes"
    );
    assert!(
        from_main
            .iter()
            .any(|hit| hit.path.as_deref() == Some("src/shared.rs")),
        "byte-identical content shared by both checkouts remains visible"
    );

    database.destroy(pool).await;
}

/// The same element body can sit inside two different file blobs. The foreign
/// blob is indexed FIRST so its element gets the lower id; the feature blob is
/// indexed SECOND and is the caller. Without a scoped representative resolver,
/// the candidate gate admits the caller-held raw hash and the later global
/// lowest-id pick detaches it from the caller, yielding null provenance.
///
/// The existing divergent functions have different raw hashes, while the
/// existing shared function uses one file blob in both roots. Neither creates
/// the load-bearing shape here: one raw hash, two blobs, caller holds only the
/// later blob.
#[tokio::test]
async fn scoped_search_resolves_the_element_held_by_the_caller() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;

    let identity =
        RepoIdentity::from_remote_parts(Some("github.com"), "AI-Substrate/flowspace3").unwrap();
    let main_root = "/srv/fs3-main";
    let feature_root = "/srv/fs3-feature";
    let main = register_worktree(&pool, &identity, main_root, Some("main"))
        .await
        .unwrap();
    let feature = register_worktree(&pool, &identity, feature_root, Some("feature"))
        .await
        .unwrap();

    let body = "mod tests { fn discovers_languages() {} }";
    // Insertion order is the reproduction: the foreign main element must win
    // the unscoped `ORDER BY el.id LIMIT 1` race in the broken query.
    let main_blob = index_file(&pool, main, "src/discovery.rs", &[body]).await;
    let feature_blob = index_file(&pool, feature, "src/discovery.rs", &[body]).await;
    assert_ne!(
        main_blob, feature_blob,
        "one element hash must occur in two distinct file blobs"
    );

    let query = vector_for("discovers languages tests").await;
    let hits = search_elements(
        &pool,
        EMBEDDER,
        &query,
        &SearchFilters {
            repo: Some(identity.key().to_string()),
            worktree: Some(feature_root.to_string()),
            ..SearchFilters::default()
        },
    )
    .await
    .unwrap()
    .hits;

    assert!(
        !hits.is_empty(),
        "the caller-held element remains searchable"
    );
    assert!(
        hits.iter().all(|hit| {
            hit.identity.as_deref() == Some(identity.key())
                && hit.root_path.as_deref() == Some(feature_root)
                && hit.path.as_deref() == Some("src/discovery.rs")
                && hit.similar.blob_sha == feature_blob.as_str()
        }),
        "the representative must be the later caller-held blob: {hits:#?}"
    );

    database.destroy(pool).await;
}

/// One summary text hash can describe different raw bodies in different
/// checkouts. The smart-content chooser must select deterministically from the
/// caller scope; choosing the globally oldest mapping makes a valid caller hit
/// disappear when the later scoped element resolver rejects the foreign body.
#[tokio::test]
async fn smart_search_chooses_the_raw_body_held_by_the_caller() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;

    let identity =
        RepoIdentity::from_remote_parts(Some("github.com"), "AI-Substrate/flowspace3").unwrap();
    let main_root = "/srv/fs3-main";
    let feature_root = "/srv/fs3-feature";
    let main = register_worktree(&pool, &identity, main_root, Some("main"))
        .await
        .unwrap();
    let feature = register_worktree(&pool, &identity, feature_root, Some("feature"))
        .await
        .unwrap();

    let main_body = "fn reconcile() { use_main_policy(); }";
    let feature_body = "fn reconcile() { use_feature_policy(); }";
    let main_blob = index_file(&pool, main, "src/reconcile.rs", &[main_body]).await;
    let feature_blob = index_file(&pool, feature, "src/reconcile.rs", &[feature_body]).await;
    assert_ne!(main_blob, feature_blob);

    let summary = Summary {
        text: "Reconciles worktree state with the index.".to_string(),
        tags: vec!["worktree".to_string(), "reconcile".to_string()],
        ..Summary::default()
    };
    let main_hash = content_hash_of(main_body);
    let feature_hash = content_hash_of(feature_body);
    // Insertion order is load-bearing in the broken query: main is the global
    // oldest mapping, while feature is the only mapping eligible to the caller.
    put_smart_content(&pool, &main_hash, SUMMARIZER, &summary)
        .await
        .unwrap();
    put_smart_content(&pool, &feature_hash, SUMMARIZER, &summary)
        .await
        .unwrap();

    let smart_hash = content_hash_of(&summary.text);
    let smart_vector = vector_for(&summary.text).await;
    put_embeddings(
        &pool,
        EMBEDDER,
        &[NewEmbedding {
            chunk_no: 0,
            source_hash: &smart_hash,
            source_kind: SourceKind::Smart,
            vector: &smart_vector,
            truncated: false,
        }],
    )
    .await
    .unwrap();

    let query = vector_for("reconciles worktree state index").await;
    let hits = search_elements(
        &pool,
        EMBEDDER,
        &query,
        &SearchFilters {
            repo: Some(identity.key().to_string()),
            worktree: Some(feature_root.to_string()),
            source: Some(SourceKind::Smart),
            ..SearchFilters::default()
        },
    )
    .await
    .unwrap()
    .hits;

    assert_eq!(
        hits.len(),
        1,
        "the caller-held smart hit must not disappear"
    );
    assert_eq!(hits[0].root_path.as_deref(), Some(feature_root));
    assert_eq!(hits[0].path.as_deref(), Some("src/reconcile.rs"));
    assert_eq!(hits[0].similar.blob_sha, feature_blob.as_str());
    assert_eq!(
        hits[0]
            .similar
            .smart
            .as_ref()
            .map(|smart| smart.text.as_str()),
        Some(summary.text.as_str())
    );

    database.destroy(pool).await;
}

/// `--path` narrows to a subtree, and the hit carries the live path back — a
/// content-layer answer is not usable until the ref layer says where it is.
#[tokio::test]
async fn a_path_filter_narrows_to_a_subtree_and_hits_carry_their_live_path() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;

    let identity = identity_of("/srv/api");
    let worktree = register_worktree(&pool, &identity, "/srv/api", None)
        .await
        .unwrap();

    let blob = unique_blob();
    upsert_element_tree(
        &pool,
        &blob,
        PARSER_VERSION,
        &file_with("crates/store/src/lib.rs", &["fn migrate_the_store() {}"]),
        |element| element.kind != ElementKind::File,
    )
    .await
    .unwrap();
    let vector = vector_for("fn migrate_the_store() {}").await;
    let hash = content_hash_of("fn migrate_the_store() {}");
    put_embeddings(
        &pool,
        EMBEDDER,
        &[NewEmbedding {
            chunk_no: 0,
            source_hash: &hash,
            source_kind: SourceKind::Raw,
            vector: &vector,
            truncated: false,
        }],
    )
    .await
    .unwrap();
    sync_worktree_files(
        &pool,
        worktree,
        &[("crates/store/src/lib.rs".to_string(), blob.clone())],
    )
    .await
    .unwrap();

    let query = vector_for("migrate the store").await;

    let matching = search_elements(
        &pool,
        EMBEDDER,
        &query,
        &SearchFilters {
            path: Some("crates/store/%".to_string()),
            ..SearchFilters::default()
        },
    )
    .await
    .unwrap()
    .hits;
    assert_eq!(matching.len(), 1);
    assert_eq!(
        matching[0].path.as_deref(),
        Some("crates/store/src/lib.rs"),
        "the hit must say where the bytes live"
    );

    let elsewhere = search_elements(
        &pool,
        EMBEDDER,
        &query,
        &SearchFilters {
            path: Some("crates/daemon/%".to_string()),
            ..SearchFilters::default()
        },
    )
    .await
    .unwrap()
    .hits;
    assert!(
        elsewhere.is_empty(),
        "a filter that matches nothing returns nothing"
    );

    database.destroy(pool).await;
}

/// `--source smart` searches summaries; `--source raw` searches code. Both
/// spaces live in one table keyed by `source_kind`, so choosing between them
/// has to be a predicate rather than a different query.
#[tokio::test]
async fn a_source_filter_chooses_which_vector_space_is_searched() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;

    let identity = identity_of("/srv/api");
    let worktree = register_worktree(&pool, &identity, "/srv/api", None)
        .await
        .unwrap();

    let body = "fn rotate(keys: &mut Vec<Key>) { keys.rotate_left(1) }";
    let blob = unique_blob();
    upsert_element_tree(
        &pool,
        &blob,
        PARSER_VERSION,
        &file_with("src/keys.rs", &[body]),
        |element| element.kind != ElementKind::File,
    )
    .await
    .unwrap();
    sync_worktree_files(
        &pool,
        worktree,
        &[("src/keys.rs".to_string(), blob.clone())],
    )
    .await
    .unwrap();

    let raw_hash = content_hash_of(body);
    let summary = Summary {
        text: "Rotates the key ring by one position.".to_string(),
        tags: vec!["keys".to_string(), "rotation".to_string()],
        ..Summary::default()
    };
    put_smart_content(&pool, &raw_hash, SUMMARIZER, &summary)
        .await
        .unwrap();

    let raw_vector = vector_for(body).await;
    let smart_vector = vector_for(&summary.text).await;
    let smart_hash = content_hash_of(&summary.text);
    put_embeddings(
        &pool,
        EMBEDDER,
        &[
            NewEmbedding {
                chunk_no: 0,
                source_hash: &raw_hash,
                source_kind: SourceKind::Raw,
                vector: &raw_vector,
                truncated: false,
            },
            NewEmbedding {
                chunk_no: 0,
                source_hash: &smart_hash,
                source_kind: SourceKind::Smart,
                vector: &smart_vector,
                truncated: false,
            },
        ],
    )
    .await
    .unwrap();

    let query = vector_for("rotates the key ring").await;

    let smart_only = search_elements(
        &pool,
        EMBEDDER,
        &query,
        &SearchFilters {
            source: Some(SourceKind::Smart),
            ..SearchFilters::default()
        },
    )
    .await
    .unwrap()
    .hits;
    assert_eq!(smart_only.len(), 1);
    assert_eq!(smart_only[0].similar.source_kind, SourceKind::Smart);
    assert_eq!(
        smart_only[0]
            .similar
            .smart
            .as_ref()
            .map(|s| s.text.as_str()),
        Some(summary.text.as_str()),
        "a smart hit resolves through text_hash back to its summary"
    );
    assert_eq!(
        smart_only[0].similar.element.raw_text, body,
        "and through raw_hash back to the code it describes"
    );

    let raw_only = search_elements(
        &pool,
        EMBEDDER,
        &query,
        &SearchFilters {
            source: Some(SourceKind::Raw),
            ..SearchFilters::default()
        },
    )
    .await
    .unwrap()
    .hits;
    assert!(
        raw_only
            .iter()
            .all(|hit| hit.similar.source_kind == SourceKind::Raw)
    );

    database.destroy(pool).await;
}

/// A distance ceiling is what `--min-score` becomes. Without it a search always
/// returns `limit` rows, however irrelevant — which is how a search surface
/// starts lying about what it knows.
#[tokio::test]
async fn a_distance_ceiling_drops_hits_that_are_merely_the_nearest() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;

    let identity = identity_of("/srv/api");
    let worktree = register_worktree(&pool, &identity, "/srv/api", None)
        .await
        .unwrap();
    index_file(
        &pool,
        worktree,
        "src/net.rs",
        &["fn open_socket(addr: SocketAddr) -> io::Result<TcpStream> { todo!() }"],
    )
    .await;

    let unrelated = vector_for("markdown heading table of contents prose").await;

    let unbounded = search_elements(&pool, EMBEDDER, &unrelated, &SearchFilters::default())
        .await
        .unwrap()
        .hits;
    assert!(
        !unbounded.is_empty(),
        "without a ceiling, the nearest row is returned however far it is"
    );

    let bounded = search_elements(
        &pool,
        EMBEDDER,
        &unrelated,
        &SearchFilters {
            max_distance: Some(0.05),
            ..SearchFilters::default()
        },
    )
    .await
    .unwrap()
    .hits;
    assert!(
        bounded.is_empty(),
        "a tight ceiling returns nothing rather than the least-bad answer"
    );

    database.destroy(pool).await;
}

fn content_hash_of(text: &str) -> String {
    fs3_core::content_hash(text.as_bytes())
}
