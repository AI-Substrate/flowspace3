//! The auto-update loop, end to end, against a fake release server (PRD req 54).
//!
//! No live GitHub call anywhere in here. The stub answers exactly the two
//! shapes the real thing does — a `releases/latest` redirect whose `Location`
//! names the newest tag, and `releases/download/<tag>/<asset>` — which is what
//! makes it a fake rather than a mock: it is a real HTTP server that a real
//! `reqwest` really talks to, and swapping in `https://github.com/...` changes
//! nothing but the base URL.
//!
//! The property that matters most here is the one that cannot be unit-tested:
//! **a binary can be replaced while a process is executing it**. That is why
//! `a_running_binary_can_be_replaced_underneath_itself` actually spawns a
//! script, swaps the file out from under the live process, and then proves both
//! that the running process finished happily on its old inode and that the path
//! now holds the new bytes.

use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use axum::extract::{Path, State};
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;
use axum::routing::get;
use fs3_core::UpdateConfig;
use fs3_daemon::reconcile::Reconcile;
use fs3_daemon::update::{Outcome, UpdateSupervisor, Updater, digest};

mod support;

/// What the stub release server is publishing right now.
#[derive(Clone)]
struct Release {
    tag: String,
    assets: Arc<Vec<(String, Vec<u8>)>>,
}

/// Serve one release the way GitHub serves one.
async fn spawn_release_server(tag: &str, assets: Vec<(String, Vec<u8>)>) -> String {
    let release = Release {
        tag: tag.to_string(),
        assets: Arc::new(assets),
    };

    let router = Router::new()
        .route(
            "/releases/latest",
            get(|State(release): State<Release>| async move {
                // GitHub answers a 302 whose Location names the tag. Reading
                // that header is the whole quota-free probe.
                (
                    StatusCode::FOUND,
                    [(
                        header::LOCATION,
                        format!("/releases/tag/{}", release.tag),
                    )],
                )
                    .into_response()
            }),
        )
        .route(
            "/releases/download/{tag}/{asset}",
            get(
                |State(release): State<Release>, Path((tag, asset)): Path<(String, String)>| async move {
                    if tag != release.tag {
                        return StatusCode::NOT_FOUND.into_response();
                    }
                    match release.assets.iter().find(|(name, _)| *name == asset) {
                        Some((_, bytes)) => bytes.clone().into_response(),
                        None => StatusCode::NOT_FOUND.into_response(),
                    }
                },
            ),
        )
        .with_state(release);

    support::spawn(router).await
}

/// A release publishing `bytes` for every triple, plus the SHA256SUMS that
/// covers them — the shape `release.yml` produces.
fn published(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    let triples = [
        "aarch64-unknown-linux-gnu",
        "x86_64-unknown-linux-gnu",
        "aarch64-apple-darwin",
    ];
    let sums = triples
        .iter()
        .map(|triple| format!("{}  flowspace3-{triple}\n", digest(bytes)))
        .collect::<String>();

    let mut assets: Vec<(String, Vec<u8>)> = triples
        .iter()
        .map(|triple| (format!("flowspace3-{triple}"), bytes.to_vec()))
        .collect();
    assets.push(("SHA256SUMS".to_string(), sums.into_bytes()));
    assets
}

fn throwaway_binary(label: &str, contents: &[u8]) -> (PathBuf, PathBuf) {
    let directory = support::temp_dir(label);
    let path = directory.join("flowspace3");
    std::fs::write(&path, contents).expect("seeding the installed binary");
    (directory, path)
}

#[tokio::test]
async fn a_newer_release_is_downloaded_verified_and_swapped_in() {
    let base = spawn_release_server("v9.9.9", published(b"the new binary")).await;
    let (_directory, installed) = throwaway_binary("update-swap", b"the old binary");

    let updater = Updater::against(&base, "0.1.0")
        .expect("building the updater")
        .at_path(installed.clone());

    let outcome = updater.run_once().await.expect("the update pass");

    assert!(
        matches!(&outcome, Outcome::Installed(version) if version.to_string() == "9.9.9"),
        "expected an install, got {outcome:?}"
    );
    assert_eq!(
        std::fs::read(&installed).expect("reading back"),
        b"the new binary"
    );
}

#[tokio::test]
async fn the_newest_release_already_running_is_a_no_op() {
    let base = spawn_release_server("v9.9.9", published(b"the new binary")).await;
    let (_directory, installed) = throwaway_binary("update-current", b"the old binary");

    let outcome = Updater::against(&base, "9.9.9")
        .expect("building the updater")
        .at_path(installed.clone())
        .run_once()
        .await
        .expect("the update pass");

    assert_eq!(outcome, Outcome::Current);
    assert_eq!(
        std::fs::read(&installed).expect("reading back"),
        b"the old binary",
        "a same-version release must not be reinstalled"
    );
}

#[tokio::test]
async fn an_older_published_release_never_walks_the_install_backwards() {
    let base = spawn_release_server("v0.0.1", published(b"an ancient binary")).await;
    let (_directory, installed) = throwaway_binary("update-downgrade", b"the current binary");

    let outcome = Updater::against(&base, "9.9.9")
        .expect("building the updater")
        .at_path(installed.clone())
        .run_once()
        .await
        .expect("the update pass");

    assert_eq!(outcome, Outcome::Current);
    assert_eq!(
        std::fs::read(&installed).expect("reading back"),
        b"the current binary"
    );
}

/// TLS plus GitHub is not verification. A release whose SHA256SUMS does not
/// describe the bytes it served is refused — and refused as NEWS, not as an
/// error, because the user is the one who needs to hear it.
#[tokio::test]
async fn an_asset_that_does_not_match_the_published_checksum_is_refused() {
    let mut assets = published(b"the new binary");
    // Serve different bytes than the checksums promise — the shape of a
    // corrupted mirror or a tampered asset.
    for (name, bytes) in &mut assets {
        if name.starts_with("flowspace3-") {
            *bytes = b"something else entirely".to_vec();
        }
    }

    let base = spawn_release_server("v9.9.9", assets).await;
    let (_directory, installed) = throwaway_binary("update-checksum", b"the old binary");

    let outcome = Updater::against(&base, "0.1.0")
        .expect("building the updater")
        .at_path(installed.clone())
        .run_once()
        .await
        .expect("a bad checksum is an outcome, not a failure");

    match outcome {
        Outcome::Blocked { reason, .. } => {
            assert!(reason.contains("SHA256SUMS"), "unexpected reason: {reason}");
            assert!(reason.contains("refusing to install"));
        }
        other => panic!("expected the download to be refused, got {other:?}"),
    }
    assert_eq!(
        std::fs::read(&installed).expect("reading back"),
        b"the old binary",
        "nothing may be swapped in once the checksum has failed"
    );
}

/// A release with no checksums file is refused — and refused as NEWS, not as an
/// error. This is not a hypothetical: every release published before this
/// feature existed has no `SHA256SUMS`, so an installation looking at one must
/// be told "there is a newer version and I cannot verify it, install it
/// yourself" rather than "the release could not be read, retryable" once a day
/// forever.
#[tokio::test]
async fn a_release_with_no_checksums_asset_is_reported_rather_than_erroring() {
    let assets: Vec<(String, Vec<u8>)> = published(b"the new binary")
        .into_iter()
        .filter(|(name, _)| name != "SHA256SUMS")
        .collect();
    let base = spawn_release_server("v9.9.9", assets).await;
    let (_directory, installed) = throwaway_binary("update-nosums", b"the old binary");

    let outcome = Updater::against(&base, "0.1.0")
        .expect("building the updater")
        .at_path(installed.clone())
        .run_once()
        .await
        .expect("an unverifiable release is an outcome, not a failure");

    match outcome {
        Outcome::Blocked { latest, reason } => {
            assert_eq!(latest.to_string(), "9.9.9");
            assert!(
                reason.contains("publishes no SHA256SUMS"),
                "unexpected reason: {reason}"
            );
            assert!(reason.contains("refusing to install it unverified"));
        }
        other => panic!("expected notify-only, got {other:?}"),
    }
    assert_eq!(
        std::fs::read(&installed).expect("reading back"),
        b"the old binary"
    );
}

/// A release that publishes checksums but no binary for THIS platform is the
/// same shape of news — the triple exists, the asset does not.
#[tokio::test]
async fn a_release_missing_this_platforms_asset_is_reported_rather_than_erroring() {
    let assets: Vec<(String, Vec<u8>)> = published(b"the new binary")
        .into_iter()
        .filter(|(name, _)| !name.starts_with("flowspace3-"))
        .collect();
    let base = spawn_release_server("v9.9.9", assets).await;
    let (_directory, installed) = throwaway_binary("update-noasset", b"the old binary");

    let outcome = Updater::against(&base, "0.1.0")
        .expect("building the updater")
        .at_path(installed.clone())
        .run_once()
        .await
        .expect("a missing asset is an outcome, not a failure");

    assert!(
        matches!(
            &outcome,
            Outcome::Blocked { reason, .. } if reason.contains("publishes no flowspace3-")
        ),
        "expected the missing asset to be named, got {outcome:?}"
    );
    assert_eq!(
        std::fs::read(&installed).expect("reading back"),
        b"the old binary"
    );
}

/// The case Jordan's own machine is in: `/usr/local/bin` is root-owned, so the
/// daemon cannot swap and must degrade to notify-only rather than failing in a
/// loop.
#[tokio::test]
async fn an_unwritable_install_directory_degrades_to_notify_only() {
    let base = spawn_release_server("v9.9.9", published(b"the new binary")).await;

    let outcome = Updater::against(&base, "0.1.0")
        .expect("building the updater")
        // A directory that does not exist is unwritable for the same reason a
        // root-owned one is, and it is the version of that state a test can
        // create without sudo.
        .at_path(PathBuf::from("/definitely/not/writable/flowspace3"))
        .run_once()
        .await
        .expect("an unwritable path is an outcome, not a failure");

    match outcome {
        Outcome::Blocked { latest, reason } => {
            assert_eq!(latest.to_string(), "9.9.9");
            assert!(
                reason.contains("not writable"),
                "unexpected reason: {reason}"
            );
        }
        other => panic!("expected notify-only, got {other:?}"),
    }
}

/// The property the whole atomic-swap design exists for. A rename over a path
/// that a live process is executing is safe: the running process keeps its old
/// inode and finishes normally, and the path holds the new bytes immediately.
///
/// Written as a shell script rather than a Rust binary so the test needs no
/// build step of its own — what is being proven is a filesystem property, not
/// anything about the executable's format.
#[cfg(unix)]
#[tokio::test]
async fn a_running_binary_can_be_replaced_underneath_itself() {
    use std::os::unix::fs::PermissionsExt as _;

    let directory = support::temp_dir("update-etxtbsy");
    let installed = directory.join("flowspace3");
    let flag = directory.join("started");

    std::fs::write(
        &installed,
        format!(
            "#!/bin/sh\ntouch {}\nsleep 2\necho old-finished\n",
            flag.display()
        ),
    )
    .expect("seeding the running binary");
    std::fs::set_permissions(&installed, std::fs::Permissions::from_mode(0o755))
        .expect("making it executable");

    // `std::process`, not `tokio::process`: the tokio `process` feature is not
    // one this workspace asks for, and a test that only works because another
    // crate happened to enable it is a test that breaks on a dependency bump.
    // Nothing is awaited after the wait below, so blocking here is free.
    let running = std::process::Command::new(&installed)
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("the throwaway binary should run");

    // Wait until it is genuinely executing: swapping before `exec` would prove
    // nothing about ETXTBSY.
    for _ in 0..200 {
        if flag.exists() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(flag.exists(), "the throwaway binary never started");

    let base = spawn_release_server("v9.9.9", published(b"#!/bin/sh\necho new\n")).await;
    let outcome = Updater::against(&base, "0.1.0")
        .expect("building the updater")
        .at_path(installed.clone())
        .run_once()
        .await
        .expect("swapping a running binary");

    assert!(
        matches!(outcome, Outcome::Installed(_)),
        "the swap must succeed while the old binary is executing: {outcome:?}"
    );

    let finished = running.wait_with_output().expect("the old process");
    assert!(
        finished.status.success(),
        "the running process must survive having its path replaced"
    );
    assert_eq!(
        String::from_utf8_lossy(&finished.stdout).trim(),
        "old-finished",
        "the running process keeps executing its ORIGINAL inode"
    );
    assert_eq!(
        std::fs::read_to_string(&installed).expect("reading back"),
        "#!/bin/sh\necho new\n"
    );
}

/// The whole loop, through Postgres: a reconcile pass installs the newer
/// binary, records it, and the user messages queue starts carrying the restart
/// steering. A second pass is a no-op — the interval has been claimed — and the
/// message does not duplicate.
#[tokio::test]
async fn a_reconcile_pass_installs_and_then_steers_through_the_message_queue() {
    let database = support::FreshDatabase::create("updateloop").await;
    let pool = database.pool().await;
    fs3_store::migrate(&pool).await.expect("migrations");

    let base = spawn_release_server("v9.9.9", published(b"the new binary")).await;
    let (_directory, installed) = throwaway_binary("update-reconcile", b"the old binary");

    let mut supervisor = UpdateSupervisor::new(pool.clone(), &UpdateConfig::default(), "0.1.0")
        .expect("building the supervisor")
        .against(&base, installed.clone())
        .expect("pointing it at the stub");

    let pass = supervisor.reconcile().await.expect("the first pass");
    assert_eq!(pass.changed, 1, "the first pass must run a check");
    assert_eq!(
        std::fs::read(&installed).expect("reading back"),
        b"the new binary"
    );

    let messages = fs3_store::live_messages(&pool).await.expect("the queue");
    assert_eq!(messages.len(), 1, "expected one steer, got {messages:?}");
    assert_eq!(messages[0].key, "update:installed:9.9.9");
    assert_eq!(messages[0].source, "update");
    assert!(messages[0].text.contains("9.9.9"));
    assert!(messages[0].next_action.contains("restart"));
    assert!(
        messages[0].created.is_some(),
        "the store stamps the creation time"
    );

    // The interval is claimed in Postgres, so the very next pass must NOT ask
    // GitHub again — this is what stops a five-second reconcile cadence from
    // becoming a five-second poll of a rate-limited endpoint.
    let second = supervisor.reconcile().await.expect("the second pass");
    assert_eq!(second.changed, 0, "the interval must suppress a re-check");
    assert_eq!(
        fs3_store::live_messages(&pool)
            .await
            .expect("the queue")
            .len(),
        1,
        "re-declaring the same message must not duplicate it"
    );

    database.destroy(pool).await;
}

/// The clear-condition, proven without any clear-condition machinery: a
/// supervisor running the version that is installed simply stops declaring the
/// message, and the queue retracts it.
#[tokio::test]
async fn the_restart_steer_clears_once_the_daemon_runs_the_installed_version() {
    let database = support::FreshDatabase::create("updateclear").await;
    let pool = database.pool().await;
    fs3_store::migrate(&pool).await.expect("migrations");

    let base = spawn_release_server("v9.9.9", published(b"the new binary")).await;
    let (_directory, installed) = throwaway_binary("update-clear", b"the old binary");

    UpdateSupervisor::new(pool.clone(), &UpdateConfig::default(), "0.1.0")
        .expect("building the supervisor")
        .against(&base, installed.clone())
        .expect("pointing it at the stub")
        .reconcile()
        .await
        .expect("the installing pass");

    assert_eq!(
        fs3_store::live_messages(&pool)
            .await
            .expect("the queue")
            .len(),
        1
    );

    // The restarted daemon: same state row, new running version.
    let mut restarted = UpdateSupervisor::new(pool.clone(), &UpdateConfig::default(), "9.9.9")
        .expect("building the supervisor")
        .against(&base, installed)
        .expect("pointing it at the stub");
    restarted.reconcile().await.expect("the post-restart pass");

    assert!(
        fs3_store::live_messages(&pool)
            .await
            .expect("the queue")
            .is_empty(),
        "the steer must clear itself once the daemon is running that version"
    );

    database.destroy(pool).await;
}

/// Auto-update off means no network and no swap — but the queue still tells the
/// truth about whatever the last check concluded.
#[tokio::test]
async fn auto_update_off_never_installs() {
    let database = support::FreshDatabase::create("updateoff").await;
    let pool = database.pool().await;
    fs3_store::migrate(&pool).await.expect("migrations");

    let base = spawn_release_server("v9.9.9", published(b"the new binary")).await;
    let (_directory, installed) = throwaway_binary("update-off", b"the old binary");

    let configuration = UpdateConfig {
        auto: false,
        ..UpdateConfig::default()
    };
    let mut supervisor = UpdateSupervisor::new(pool.clone(), &configuration, "0.1.0")
        .expect("building the supervisor")
        .against(&base, installed.clone())
        .expect("pointing it at the stub");

    let pass = supervisor.reconcile().await.expect("the pass");

    assert_eq!(pass.changed, 0);
    assert_eq!(
        std::fs::read(&installed).expect("reading back"),
        b"the old binary",
        "auto = false must never swap a binary"
    );
    assert!(
        fs3_store::live_messages(&pool)
            .await
            .expect("the queue")
            .is_empty()
    );

    database.destroy(pool).await;
}
