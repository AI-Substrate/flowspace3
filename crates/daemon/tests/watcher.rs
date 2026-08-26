//! The live watcher, proven end to end: register a root, edit a file on disk,
//! watch a job appear and an element land — with no HTTP call in between.
//!
//! One throwaway database per test, the fake providers, the REAL supervisor and
//! the REAL runner. What it proves is Jordan's ask in full: *"the daemon should
//! automatically watch whatever paths are present on boot, and also if I add a
//! path, it should start watching it."*
//!
//! # Why the reconcile passes are driven by hand
//!
//! `reconcile::run_forever` is a `tokio::spawn`ed loop with a five-second
//! cadence. Spawning it here would make every assertion a race against a timer.
//! Calling `reconcile()` directly is the same code the loop calls, one pass at
//! a time, so the test states *what a pass does* rather than *how fast the loop
//! is* — and the loop's own behaviour (boot pass, error containment) is pinned
//! by unit tests in `reconcile.rs`.
//!
//! # Why the debounce is one second here
//!
//! `indexing.debounce_seconds` ships at 10. Turning it down is not a test
//! convenience: it is the assertion that the setting is READ AT ALL, which it
//! was not before this landed — the field existed in `fs3_core::IndexingConfig`
//! with no reader.

mod support;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use fs3_core::{Config, DatabaseConfig, IndexingConfig};
use fs3_daemon::reconcile::Reconcile;
use fs3_daemon::watch::WatcherSupervisor;
use fs3_daemon::wiring::AppState;
use fs3_daemon::{roots, runner};
use sqlx::Row;

/// Long enough for a real OS watcher to deliver, short enough to fail fast.
const PATIENCE: Duration = Duration::from_secs(20);

/// A throwaway database, the fake stack, and a scratch tree to watch.
struct Stack {
    database: support::FreshDatabase,
    state: AppState,
    root: PathBuf,
}

impl Stack {
    async fn create(label: &str) -> Self {
        let database = support::FreshDatabase::create(label).await;
        let config = Config {
            database: DatabaseConfig {
                url: database.url(),
            },
            indexing: IndexingConfig {
                debounce_seconds: 1,
                ..IndexingConfig::default()
            },
            ..Config::default()
        };
        let state = AppState::from_config(config).expect("the fake stack wires");
        fs3_store::migrate(&state.db)
            .await
            .expect("a fresh database migrates");

        let root = support::temp_dir(label);
        Self {
            database,
            state,
            root,
        }
    }

    /// Write a file, creating its parents. Not git-backed on purpose: a plain
    /// directory exercises the `IdentitySource::Path` arm, and `blob_id` is
    /// git's hash function rather than a repository operation.
    fn write(&self, relative: &str, contents: &str) {
        let path = self.root.join(relative);
        std::fs::create_dir_all(path.parent().expect("a file has a parent"))
            .expect("creating parents");
        std::fs::write(&path, contents).expect("writing a fixture file");
    }

    async fn add_root(&self) -> roots::RootReport {
        roots::add_root(&self.state, &self.root)
            .await
            .expect("registering the scratch root")
    }

    /// Drain the queue with the REAL runner until nothing is ready.
    async fn drain(&self) -> runner::Drained {
        let mut total = runner::Drained::default();
        for _ in 0..8 {
            let pass = runner::drain(&self.state, 4).await;
            if pass.total() == 0 {
                break;
            }
            total.completed += pass.completed;
            total.retried += pass.retried;
            total.failed += pass.failed;
        }
        total
    }

    async fn count(&self, sql: &str) -> i64 {
        sqlx::query(sql)
            .fetch_one(&self.state.db)
            .await
            .expect("a count query")
            .try_get::<i64, _>(0)
            .expect("counts are bigints")
    }

    /// How many scan jobs are waiting or running.
    async fn pending_scans(&self) -> i64 {
        self.count(
            "SELECT count(*) FROM jobs WHERE kind = 'scan_file' AND state IN ('pending', 'running')",
        )
        .await
    }

    /// Whether an element was minted for this worktree-relative path.
    async fn elements_for(&self, relative: &str) -> i64 {
        self.count(&format!(
            "SELECT count(*) FROM elements WHERE address LIKE '{relative}%'"
        ))
        .await
    }

    async fn destroy(self) {
        let pool = self.state.db.clone();
        self.database.destroy(pool).await;
    }
}

/// Run reconcile passes until `done`, or give up.
///
/// Polling rather than sleeping a fixed time: the claim is "a pass eventually
/// sees it", and a real OS watcher plus a one-second debounce has no schedule
/// worth asserting.
async fn reconcile_until<F>(
    supervisor: &mut WatcherSupervisor,
    limit: Duration,
    mut done: F,
) -> bool
where
    F: AsyncFnMut() -> bool,
{
    let deadline = Instant::now() + limit;
    loop {
        supervisor
            .reconcile()
            .await
            .expect("a reconcile pass against a live database");
        if done().await {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

/// The headline: a file edited on disk becomes a job and then an element, with
/// nothing but the watcher in between.
#[tokio::test]
async fn a_file_written_under_a_watched_root_is_scanned_without_being_asked() {
    let stack = Stack::create("watch-live").await;
    stack.write("src/first.rs", "/// One.\npub fn first() -> u8 { 1 }\n");

    let report = stack.add_root().await;
    assert_eq!(report.files, 1);
    assert_eq!(report.enqueued, 1);
    assert_eq!(stack.drain().await.failed, 0);
    assert!(stack.elements_for("src/first.rs").await > 0);
    assert_eq!(stack.pending_scans().await, 0, "the queue starts drained");

    // The supervisor is built AFTER the root is registered, so its first pass
    // is the boot case: watch what the table already says.
    let mut supervisor = WatcherSupervisor::new(stack.state.clone());
    supervisor
        .reconcile()
        .await
        .expect("the boot pass installs a watcher");

    stack.write("src/second.rs", "/// Two.\npub fn second() -> u8 { 2 }\n");

    let saw_it = reconcile_until(&mut supervisor, PATIENCE, async || {
        stack.pending_scans().await > 0
    })
    .await;
    assert!(saw_it, "the watcher never enqueued a scan for the new file");

    assert_eq!(stack.drain().await.failed, 0);
    assert!(
        stack.elements_for("src/second.rs").await > 0,
        "a file nobody asked about was indexed because it changed on disk"
    );

    stack.destroy().await;
}

/// The other half of Jordan's ask: a root added while the daemon is already
/// running starts being watched, with no restart and no special case.
///
/// Note the ordering, which is the contract rather than test choreography.
/// `add` walks the tree and enqueues everything it finds, so files present at
/// add time are covered by `add` itself. The WATCHER covers what changes
/// after it is installed — and it is installed by the next reconcile pass, up
/// to one cadence later. An edit landing inside that window is seen by
/// neither, which is a named gap in `docs/services/watcher.md` and not
/// something this test should pretend away by writing the file early.
#[tokio::test]
async fn a_root_added_after_the_supervisor_exists_starts_being_watched() {
    let stack = Stack::create("watch-add").await;
    stack.write("src/first.rs", "/// One.\npub fn first() -> u8 { 1 }\n");

    // Supervisor first, root second — the reverse of the boot case, and the
    // same code path.
    let mut supervisor = WatcherSupervisor::new(stack.state.clone());
    supervisor
        .reconcile()
        .await
        .expect("a pass with nothing registered");

    stack.add_root().await;
    stack.drain().await;
    assert_eq!(stack.pending_scans().await, 0, "add's own work is finished");

    // ONE pass. If a runtime-added root were not picked up, no watcher would
    // ever exist and the poll below could only time out.
    supervisor
        .reconcile()
        .await
        .expect("the pass that notices the new root");

    stack.write("src/late.rs", "/// Late.\npub fn late() -> u8 { 3 }\n");

    let saw_it = reconcile_until(&mut supervisor, PATIENCE, async || {
        stack.pending_scans().await > 0
    })
    .await;
    assert!(saw_it, "a root added at runtime was never picked up");

    stack.drain().await;
    assert!(stack.elements_for("src/late.rs").await > 0);

    stack.destroy().await;
}

/// The inotify finding, as an end-to-end claim: files created inside a
/// brand-new directory yield no per-file events on Linux, and the daemon has to
/// index them anyway.
///
/// It passes on macOS for a different reason than on Linux — FSEvents reports
/// the files, inotify reports only the directory — which is the point. The
/// daemon's contract is "the directory was dirty, so re-list it", and that is
/// true on both.
#[tokio::test]
async fn files_created_inside_a_brand_new_directory_are_indexed_anyway() {
    let stack = Stack::create("watch-newdir").await;
    stack.write("src/first.rs", "/// One.\npub fn first() -> u8 { 1 }\n");
    stack.add_root().await;
    stack.drain().await;

    let mut supervisor = WatcherSupervisor::new(stack.state.clone());
    supervisor.reconcile().await.expect("the boot pass");

    // Directory and contents in one breath: the git-clone / npm-install shape.
    for index in 0..5 {
        stack.write(
            &format!("fresh/f{index}.rs"),
            &format!("/// Fresh {index}.\npub fn f{index}() -> u8 {{ {index} }}\n"),
        );
    }

    let indexed = reconcile_until(&mut supervisor, PATIENCE, async || {
        // Drain as we go: the assertion is about elements, and the jobs may
        // arrive across more than one pass.
        stack.drain().await;
        stack.elements_for("fresh/f4.rs").await > 0
    })
    .await;

    assert!(
        indexed,
        "a directory created and filled in one breath must still be indexed — \
         on inotify the per-file events do not exist and only the directory event survives"
    );
    for index in 0..5 {
        assert!(
            stack.elements_for(&format!("fresh/f{index}.rs")).await > 0,
            "every file in the new directory, not just the one that raced last"
        );
    }

    stack.destroy().await;
}

/// Churn inside `.git` is the loudest thing on a developer's disk and means
/// nothing to the index. It must not buy a single directory walk.
#[tokio::test]
async fn writes_inside_ignored_directories_never_become_work() {
    let stack = Stack::create("watch-ignored").await;
    stack.write("src/first.rs", "/// One.\npub fn first() -> u8 { 1 }\n");
    stack.add_root().await;
    stack.drain().await;

    let mut supervisor = WatcherSupervisor::new(stack.state.clone());
    supervisor.reconcile().await.expect("the boot pass");

    for index in 0..10 {
        stack.write(&format!(".git/objects/o{index}"), "noise");
        stack.write(&format!("target/debug/b{index}"), "noise");
        stack.write(&format!("node_modules/pkg/m{index}.js"), "noise");
    }

    // Several passes across more than one debounce window: if the filter
    // leaked, this is where a job would show up.
    let deadline = Instant::now() + Duration::from_secs(4);
    while Instant::now() < deadline {
        supervisor.reconcile().await.expect("a pass");
        assert_eq!(
            stack.pending_scans().await,
            0,
            "an ignored directory must never enqueue work"
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    stack.destroy().await;
}

/// The property that makes over-reporting free, at the level the watcher works
/// at: a directory that is dirty but whose CONTENT did not change enqueues
/// nothing at all.
#[tokio::test]
async fn a_dirty_directory_whose_content_is_unchanged_enqueues_nothing() {
    let stack = Stack::create("watch-unchanged").await;
    let body = "/// One.\npub fn first() -> u8 { 1 }\n";
    stack.write("src/first.rs", body);
    stack.add_root().await;
    stack.drain().await;
    assert_eq!(stack.pending_scans().await, 0);

    let mut supervisor = WatcherSupervisor::new(stack.state.clone());
    supervisor.reconcile().await.expect("the boot pass");

    // A write that changes the mtime and nothing else. The watcher sees an
    // event; the blob diff sees the same bytes.
    stack.write("src/first.rs", body);

    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        supervisor.reconcile().await.expect("a pass");
        assert_eq!(
            stack.pending_scans().await,
            0,
            "content keying is what makes an extra directory listing free"
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    stack.destroy().await;
}

/// A root registered at a path that has since been deleted must not stop the
/// pass — the loop's whole value is that the next one tries again.
#[tokio::test]
async fn a_root_that_vanished_from_disk_does_not_break_the_pass() {
    let stack = Stack::create("watch-vanished").await;
    stack.write("src/first.rs", "/// One.\npub fn first() -> u8 { 1 }\n");
    stack.add_root().await;
    stack.drain().await;

    std::fs::remove_dir_all(&stack.root).expect("removing the watched tree");

    let mut supervisor = WatcherSupervisor::new(stack.state.clone());
    for _ in 0..3 {
        supervisor
            .reconcile()
            .await
            .expect("a missing root is a warning, never a failed pass");
    }

    stack.destroy().await;
}

/// Discovery's rules still apply: the watcher decides WHEN to look, not WHAT
/// counts. A file discovery refuses is not indexed just because it moved.
#[tokio::test]
async fn a_file_discovery_refuses_is_not_indexed_just_because_it_changed() {
    let stack = Stack::create("watch-refused").await;
    stack.write("src/first.rs", "/// One.\npub fn first() -> u8 { 1 }\n");
    stack.add_root().await;
    stack.drain().await;

    let mut supervisor = WatcherSupervisor::new(stack.state.clone());
    supervisor.reconcile().await.expect("the boot pass");

    // An extension fs3 has no grammar for and does not index.
    stack.write("src/notes.bin", "\u{0}\u{1}\u{2}not source at all");

    let deadline = Instant::now() + Duration::from_secs(4);
    while Instant::now() < deadline {
        supervisor.reconcile().await.expect("a pass");
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    stack.drain().await;

    assert_eq!(
        stack.elements_for("src/notes.bin").await,
        0,
        "`discover` owns what is worth scanning; the watcher only owns when to ask"
    );

    stack.destroy().await;
}

/// Not exercised end to end, and named so the gap is visible rather than
/// assumed: there is no store API to UNREGISTER a worktree, so the stop half of
/// the root diff cannot be driven through the real stack yet. It is pinned by
/// `watch::tests::a_root_no_longer_registered_is_stopped` against the pure
/// diff, and it joins the queued decisions in `docs/services/watcher.md`.
#[allow(dead_code)]
fn stop_path_is_unit_tested_only(_: &Path) {}
