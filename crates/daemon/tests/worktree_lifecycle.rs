//! Worktree creation and removal through the real supervisor and store paths.
//!
//! Passes are driven by hand: the runner's cadence is tested separately, while
//! this test proves exactly what one scheduled pass changes.

mod support;

use std::path::Path;
use std::process::Command;

use fs3_core::{Config, DatabaseConfig, IndexingConfig};
use fs3_daemon::reconcile::Reconcile;
use fs3_daemon::wiring::AppState;
use fs3_daemon::worktrees::WorktreeSupervisor;
use fs3_daemon::{roots, runner};

#[tokio::test]
async fn linked_worktrees_reuse_current_blobs_and_scan_only_divergence() {
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
    let identical = fixture.path().join("identical tree");
    let divergent = fixture.path().join("divergent tree");
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
            "identical-test",
            identical.to_str().unwrap(),
            "HEAD",
        ],
    );
    git(
        &main,
        &[
            "worktree",
            "add",
            "-b",
            "divergent-test",
            divergent.to_str().unwrap(),
            "HEAD",
        ],
    );
    std::fs::write(divergent.join("lib.rs"), "pub fn divergent_version() {}\n").unwrap();

    roots::add_root(&state, &main, None)
        .await
        .expect("main root registers");
    runner::drain(&state, 2).await;
    for index in 0..3 {
        fs3_store::enqueue_job(
            &state.db,
            roots::SCAN_FILE,
            &format!("scan:backlog:{index}"),
            &serde_json::json!({ "backlog": index }),
            std::time::Duration::ZERO,
        )
        .await
        .expect("seeding an older normal-priority backlog");
    }
    let jobs_before = count_scan_jobs(&state).await;
    let mut supervisor = WorktreeSupervisor::new(state.clone());

    let discovered = supervisor.reconcile().await.expect("discovery pass");
    assert_eq!(discovered.changed, 2);
    let identical = identical.canonicalize().expect("identical root resolves");
    let divergent = divergent.canonicalize().expect("divergent root resolves");
    let identical_row = fs3_store::find_worktree(&state.db, &identical.display().to_string())
        .await
        .unwrap()
        .expect("the identical root is registered");
    let divergent_row = fs3_store::find_worktree(&state.db, &divergent.display().to_string())
        .await
        .unwrap()
        .expect("the divergent root is registered");

    let main_row = fs3_store::find_worktree(
        &state.db,
        &main.canonicalize().unwrap().display().to_string(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        fs3_store::worktree_file_map(&state.db, identical_row.id)
            .await
            .unwrap(),
        fs3_store::worktree_file_map(&state.db, main_row.id)
            .await
            .unwrap(),
        "registration must sync mappings even when every scan is reusable"
    );
    assert_eq!(
        scan_paths_for(&state, identical_row.id).await,
        Vec::<String>::new(),
        "an identical checkout must mint no scan jobs"
    );
    assert_eq!(
        scan_paths_for(&state, divergent_row.id).await,
        vec!["lib.rs"],
        "only the divergent file needs scan work"
    );
    assert_eq!(count_scan_jobs(&state).await, jobs_before + 1);

    let claimed = fs3_store::claim_job(&state.db, &[roots::SCAN_FILE])
        .await
        .unwrap()
        .expect("the divergent scan is ready");
    let claimed_scan: roots::ScanFileJob = serde_json::from_value(claimed.payload).unwrap();
    assert_eq!(claimed_scan.worktree_id, divergent_row.id);
    assert_eq!(claimed_scan.path, "lib.rs");

    let jobs_before = count_scan_jobs(&state).await;
    let unchanged = supervisor.reconcile().await.expect("steady-state pass");
    assert_eq!(unchanged.changed, 0);
    assert_eq!(
        count_scan_jobs(&state).await,
        jobs_before,
        "an unchanged pass must enqueue zero jobs"
    );

    git(
        &main,
        &["worktree", "remove", "--force", identical.to_str().unwrap()],
    );
    let first_absence = supervisor.reconcile().await.expect("first absent pass");
    assert_eq!(first_absence.changed, 0);
    assert!(
        fs3_store::find_worktree(&state.db, &identical.display().to_string())
            .await
            .unwrap()
            .is_some(),
        "one transient absence must not unregister paid content"
    );

    let removed = supervisor.reconcile().await.expect("second absent pass");
    assert_eq!(removed.changed, 1);
    assert!(
        fs3_store::find_worktree(&state.db, &identical.display().to_string())
            .await
            .unwrap()
            .is_none()
    );
}

async fn count_scan_jobs(state: &AppState) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM jobs WHERE kind = $1")
        .bind(roots::SCAN_FILE)
        .fetch_one(&state.db)
        .await
        .expect("counting scan jobs")
}

async fn scan_paths_for(state: &AppState, worktree_id: i64) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT payload->>'path' FROM jobs
          WHERE kind = $1 AND payload->>'worktree_id' = $2
          ORDER BY id",
    )
    .bind(roots::SCAN_FILE)
    .bind(worktree_id.to_string())
    .fetch_all(&state.db)
    .await
    .expect("reading worktree scan paths")
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
