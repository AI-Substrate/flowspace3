//! Removing a root while it is being indexed (PRD req 57).
//!
//! Jordan ruled mid-scan removal first-class: "we should kill the job queue for
//! that thing and make sure no more are processed too." These prove it end to
//! end against a real database and the real runner, because every interesting
//! part of it is a race between components that a unit test cannot stage.
//!
//! The pleasant surprise, recorded here because it was a real fear worth
//! retiring: there is no lock protocol to get right. `claim_job` takes its row
//! lock inside ONE autocommit statement, so a running job's row is MARKED
//! running, not held — the removal's delete never waits on a worker.

use fs3_core::Config;
use fs3_daemon::reconcile::Reconcile;
use fs3_daemon::{AppState, WatcherSupervisor};

mod support;

/// A config wired entirely to the offline fakes, pointed at `database_url`.
fn offline(database_url: &str) -> Config {
    Config::from_toml_str(&format!(
        r#"
        [database]
        url = "{database_url}"

        [embedder]
        active = "fake"

        [summarizer]
        active = "fake"
        "#
    ))
    .expect("the offline configuration must parse")
}

/// Write `count` tiny Rust files into a fresh directory.
fn a_repo_of(count: usize, label: &str) -> std::path::PathBuf {
    // CANONICALISED, because `add` registers the resolved path and `remove`
    // matches on it exactly. On macOS `/var` is a symlink to `/private/var`, so
    // a test that kept the unresolved form would ask the daemon to remove a
    // root it never stored — which is the same trap a user hits, and the reason
    // the not-registered envelope now lists the paths that ARE registered.
    let directory = support::temp_dir(label);
    for index in 0..count {
        std::fs::write(
            directory.join(format!("f{index}.rs")),
            format!("fn f{index}() {{ body_{index}() }}\n"),
        )
        .expect("seeding a file");
    }
    std::fs::canonicalize(&directory).unwrap_or(directory)
}

/// How many live (pending or running) jobs the queue holds for `worktree`.
async fn live_jobs_for(pool: &fs3_store::PgPool, worktree: i64) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM jobs
          WHERE state IN ('pending', 'running')
            AND payload->>'worktree_id' = $1",
    )
    .bind(worktree.to_string())
    .fetch_one(pool)
    .await
    .expect("counting live jobs")
}

/// The whole ruling, in one test: a root removed mid-index takes its queued
/// work with it, and nothing re-creates it afterwards.
#[tokio::test]
async fn removing_a_root_mid_scan_kills_its_queue_and_nothing_comes_back() {
    let database = support::FreshDatabase::create("removemidscan").await;
    let pool = database.pool().await;
    fs3_store::migrate(&pool).await.expect("migrations");

    let state = AppState::from_config(offline(&database.url())).expect("wiring");
    let directory = a_repo_of(40, "remove-mid-scan");
    let root = directory.to_string_lossy().to_string();

    let report = fs3_daemon::roots::add_root(&state, &directory, None)
        .await
        .expect("adding the root");
    let worktree = report.worktree_id;
    assert!(report.enqueued > 0, "the root must have queued real work");

    // Mid-flight on purpose: some jobs done, plenty still queued. Removing a
    // root whose queue had already drained would prove nothing.
    assert!(
        live_jobs_for(&pool, worktree).await > 0,
        "the queue must still hold work when the removal lands"
    );

    let removal = fs3_daemon::remove::remove(&state, &root)
        .await
        .expect("removing mid-scan");
    assert!(removal.was_registered);
    assert!(
        removal.jobs_killed > 0,
        "the removal must have taken real queued work with it: {removal:?}"
    );

    assert_eq!(
        live_jobs_for(&pool, worktree).await,
        0,
        "no live job may survive the removal transaction"
    );

    // "make sure no more are processed too": drain the runner repeatedly. A
    // job that reappeared — from a late discovery result, a re-emission, or a
    // watcher hint — would show up here.
    for pass in 0..3 {
        fs3_daemon::drain(&state, 2).await;
        assert_eq!(
            live_jobs_for(&pool, worktree).await,
            0,
            "work reappeared for a removed root on pass {pass}"
        );
    }

    // And the registration really is gone, not merely emptied.
    assert!(
        !fs3_store::worktree_exists(&pool, worktree)
            .await
            .expect("checking")
    );

    std::fs::remove_dir_all(&directory).ok();
    database.destroy(pool).await;
}

/// A job claimed BEFORE the removal must settle harmlessly — no foreign-key
/// spray, no resurrection of the root, no failed row.
///
/// The scan worker already re-reads its worktree and no-ops when it is gone;
/// this pins that behaviour down so a future refactor cannot quietly drop it.
#[tokio::test]
async fn a_scan_claimed_before_the_removal_settles_without_complaint() {
    let database = support::FreshDatabase::create("removeclaimed").await;
    let pool = database.pool().await;
    fs3_store::migrate(&pool).await.expect("migrations");

    let state = AppState::from_config(offline(&database.url())).expect("wiring");
    let directory = a_repo_of(4, "remove-claimed");
    let root = directory.to_string_lossy().to_string();

    let report = fs3_daemon::roots::add_root(&state, &directory, None)
        .await
        .expect("adding the root");

    // Claim a scan the way a worker would, and hold it while the root goes.
    let claimed = fs3_store::claim_job(&pool, &["scan_file"])
        .await
        .expect("claiming")
        .expect("a scan should be ready");

    fs3_daemon::remove::remove(&state, &root)
        .await
        .expect("removing under the claim");

    // The worker finishes what it started. It must not error, and it must not
    // write anything that re-creates the root.
    fs3_daemon::scan::run(&state, claimed.payload.clone())
        .await
        .expect("a scan whose root has gone is a no-op, not a failure");

    // Settling a job whose row the removal deleted must also be quiet.
    fs3_store::complete_job(&pool, claimed.id)
        .await
        .expect("settling a deleted job must not error");

    assert!(
        !fs3_store::worktree_exists(&pool, report.worktree_id)
            .await
            .expect("checking"),
        "the in-flight job must not resurrect the root"
    );
    let failed: i64 = sqlx::query_scalar("SELECT count(*) FROM jobs WHERE state = 'failed'")
        .fetch_one(&pool)
        .await
        .expect("counting failures");
    assert_eq!(failed, 0, "no error spray");

    std::fs::remove_dir_all(&directory).ok();
    database.destroy(pool).await;
}

/// The watcher drops a removed root within one reconcile pass — level-triggered,
/// so it needs no notification that a removal happened.
#[tokio::test]
async fn the_watcher_stops_watching_a_removed_root_within_one_pass() {
    let database = support::FreshDatabase::create("removewatcher").await;
    let pool = database.pool().await;
    fs3_store::migrate(&pool).await.expect("migrations");

    let state = AppState::from_config(offline(&database.url())).expect("wiring");
    let directory = a_repo_of(2, "remove-watcher");
    let root = directory.to_string_lossy().to_string();

    fs3_daemon::roots::add_root(&state, &directory, None)
        .await
        .expect("adding the root");

    let mut watcher = WatcherSupervisor::new(state.clone());
    watcher.reconcile().await.expect("the first pass");
    assert_eq!(
        watcher.watched_roots(),
        vec![directory.clone()],
        "the root must be watched before it is removed"
    );

    fs3_daemon::remove::remove(&state, &root)
        .await
        .expect("removing");

    watcher.reconcile().await.expect("the pass after removal");
    assert!(
        watcher.watched_roots().is_empty(),
        "one pass must be enough — the watcher reads Postgres, not events"
    );

    std::fs::remove_dir_all(&directory).ok();
    database.destroy(pool).await;
}

/// Removing one of two roots leaves the other watched and indexed. The
/// obvious-but-essential negative: a removal that took out a bystander would
/// pass every test above.
#[tokio::test]
async fn removing_one_root_leaves_the_other_alone() {
    let database = support::FreshDatabase::create("removeneighbour").await;
    let pool = database.pool().await;
    fs3_store::migrate(&pool).await.expect("migrations");

    let state = AppState::from_config(offline(&database.url())).expect("wiring");
    let doomed = a_repo_of(3, "remove-doomed");
    let kept = a_repo_of(3, "remove-kept");

    fs3_daemon::roots::add_root(&state, &doomed, None)
        .await
        .expect("adding");
    let survivor = fs3_daemon::roots::add_root(&state, &kept, None)
        .await
        .expect("adding")
        .worktree_id;

    fs3_daemon::remove::remove(&state, &doomed.to_string_lossy())
        .await
        .expect("removing");

    assert!(
        fs3_store::worktree_exists(&pool, survivor)
            .await
            .expect("checking"),
        "the neighbour must survive"
    );
    let files = fs3_store::worktree_file_map(&pool, survivor)
        .await
        .expect("reading the map");
    assert!(!files.is_empty(), "and keep its file map");

    let mut watcher = WatcherSupervisor::new(state.clone());
    watcher.reconcile().await.expect("a pass");
    assert_eq!(
        watcher.watched_roots(),
        vec![kept.clone()],
        "and stay watched"
    );

    // Its content is not garbage, because it is still referenced.
    let reclaimable = fs3_store::reclaimable(&pool).await.expect("counting");
    let after = fs3_store::collect_garbage(&pool).await.expect("collecting");
    assert!(
        after.elements >= reclaimable.elements,
        "GC reclaims at least the floor it reported"
    );
    assert!(
        !fs3_store::worktree_file_map(&pool, survivor)
            .await
            .expect("reading the map")
            .is_empty(),
        "and never touches the survivor's rows"
    );

    std::fs::remove_dir_all(&doomed).ok();
    std::fs::remove_dir_all(&kept).ok();
    database.destroy(pool).await;
}

/// Sanity on the timing claim: two removals in flight at once must not
/// deadlock or double-count. Cheap to run, and the kind of thing that only
/// fails on somebody else's machine.
#[tokio::test]
async fn concurrent_removals_do_not_deadlock() {
    let database = support::FreshDatabase::create("removeconcurrent").await;
    let pool = database.pool().await;
    fs3_store::migrate(&pool).await.expect("migrations");

    let state = AppState::from_config(offline(&database.url())).expect("wiring");
    let directory = a_repo_of(5, "remove-concurrent");
    let root = directory.to_string_lossy().to_string();
    fs3_daemon::roots::add_root(&state, &directory, None)
        .await
        .expect("adding");

    let (first, second) = tokio::join!(
        fs3_daemon::remove::remove(&state, &root),
        fs3_daemon::remove::remove(&state, &root),
    );

    let first = first.expect("the first removal");
    let second = second.expect("the second removal");
    assert!(
        first.was_registered ^ second.was_registered,
        "exactly one may claim the removal: {first:?} / {second:?}"
    );

    std::fs::remove_dir_all(&directory).ok();
    database.destroy(pool).await;
}

#[tokio::test]
async fn gc_leaves_a_live_index_completely_alone() {
    let database = support::FreshDatabase::create("gcnoop").await;
    let pool = database.pool().await;
    fs3_store::migrate(&pool).await.expect("migrations");

    let state = AppState::from_config(offline(&database.url())).expect("wiring");
    let directory = a_repo_of(6, "gc-noop");
    fs3_daemon::roots::add_root(&state, &directory, None)
        .await
        .expect("adding");
    fs3_daemon::drain(&state, 2).await;

    let before: i64 = sqlx::query_scalar("SELECT count(*) FROM elements")
        .fetch_one(&pool)
        .await
        .expect("counting");
    assert!(before > 0, "the index must have content to protect");

    let reclaimed = fs3_store::collect_garbage(&pool).await.expect("collecting");
    assert!(
        reclaimed.is_empty(),
        "a healthy index has no garbage: {reclaimed:?}"
    );

    let after: i64 = sqlx::query_scalar("SELECT count(*) FROM elements")
        .fetch_one(&pool)
        .await
        .expect("counting");
    assert_eq!(before, after, "GC must not touch a referenced row");

    std::fs::remove_dir_all(&directory).ok();
    database.destroy(pool).await;
}
