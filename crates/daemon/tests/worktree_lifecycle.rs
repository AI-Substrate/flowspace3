//! Worktree creation and removal through the real supervisor and store paths.
//!
//! Passes are driven by hand: the runner's cadence is tested separately, while
//! this test proves exactly what one scheduled pass changes.

mod support;

use std::path::Path;
use std::process::Command;

use fs3_core::{Config, DatabaseConfig, IndexingConfig};
use fs3_daemon::reconcile::Reconcile;
use fs3_daemon::roots;
use fs3_daemon::wiring::AppState;
use fs3_daemon::worktrees::WorktreeSupervisor;
use sqlx::Row;

#[tokio::test]
async fn linked_worktrees_are_registered_once_and_removed_after_two_absences() {
    let database = support::FreshDatabase::create("worktree-lifecycle").await;
    let state = AppState::from_config(Config {
        database: DatabaseConfig {
            url: database.url(),
        },
        indexing: IndexingConfig {
            worktree_reconcile_ticks: 1,
            ..IndexingConfig::default()
        },
        ..Config::default()
    })
    .expect("the fake stack wires");
    fs3_store::migrate(&state.db)
        .await
        .expect("a fresh database migrates");

    let fixture = tempfile::tempdir().expect("fixture directory");
    let main = fixture.path().join("main");
    let linked = fixture.path().join("linked tree");
    git(
        fixture.path(),
        &["init", "--initial-branch=main", main.to_str().unwrap()],
    );
    git(&main, &["config", "user.email", "test@example.com"]);
    git(&main, &["config", "user.name", "Test"]);
    std::fs::write(main.join("lib.rs"), "pub fn main_version() {}\n").unwrap();
    git(&main, &["add", "lib.rs"]);
    git(&main, &["commit", "-m", "fixture"]);
    git(
        &main,
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/example/worktree-lifecycle.git",
        ],
    );
    git(
        &main,
        &[
            "worktree",
            "add",
            "-b",
            "linked-test",
            linked.to_str().unwrap(),
            "HEAD",
        ],
    );

    roots::add_root(&state, &main)
        .await
        .expect("main root registers");
    let mut supervisor = WorktreeSupervisor::new(state.clone());

    let discovered = supervisor.reconcile().await.expect("discovery pass");
    assert_eq!(discovered.changed, 1);
    let linked = linked.canonicalize().expect("linked root resolves");
    assert!(
        fs3_store::find_worktree(&state.db, &linked.display().to_string())
            .await
            .unwrap()
            .is_some()
    );

    let jobs_before = count_jobs(&state).await;
    let unchanged = supervisor.reconcile().await.expect("steady-state pass");
    assert_eq!(unchanged.changed, 0);
    assert_eq!(
        count_jobs(&state).await,
        jobs_before,
        "an unchanged pass must enqueue zero jobs"
    );

    git(
        &main,
        &["worktree", "remove", "--force", linked.to_str().unwrap()],
    );
    let first_absence = supervisor.reconcile().await.expect("first absent pass");
    assert_eq!(first_absence.changed, 0);
    assert!(
        fs3_store::find_worktree(&state.db, &linked.display().to_string())
            .await
            .unwrap()
            .is_some(),
        "one transient absence must not unregister paid content"
    );

    let removed = supervisor.reconcile().await.expect("second absent pass");
    assert_eq!(removed.changed, 1);
    assert!(
        fs3_store::find_worktree(&state.db, &linked.display().to_string())
            .await
            .unwrap()
            .is_none()
    );
}

async fn count_jobs(state: &AppState) -> i64 {
    sqlx::query("SELECT count(*) FROM jobs")
        .fetch_one(&state.db)
        .await
        .expect("counting jobs")
        .try_get(0)
        .expect("count is bigint")
}

fn git(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .env("LC_ALL", "C")
        .output()
        .expect("git runs");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
