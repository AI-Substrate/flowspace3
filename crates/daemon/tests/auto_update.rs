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

/// A stand-in binary that answers `--version` the way the real one does.
///
/// A shell script rather than a compiled artefact: the updater now RUNS what it
/// downloaded before installing it (req-0060's loop guard), so an asset that is
/// merely bytes is no longer a realistic fake. What is being modelled is "a
/// thing that execs and reports a version", and a script is exactly that
/// without adding a build step to the test suite.
fn binary_reporting(version: &str) -> Vec<u8> {
    format!("#!/bin/sh\necho \"flowspace3 {version}\"\n").into_bytes()
}

fn throwaway_binary(label: &str, contents: &[u8]) -> (PathBuf, PathBuf) {
    let directory = support::temp_dir(label);
    let path = directory.join("flowspace3");
    std::fs::write(&path, contents).expect("seeding the installed binary");
    (directory, path)
}

/// A throwaway binary that ANSWERS `--version`, so the disk reconciliation has
/// something truthful to read at the install path.
///
/// `b"the old binary"` was fine when the row remembered what the updater did;
/// it is not fine now that the row is a reading of the file, because a pile of
/// bytes that cannot exec is indistinguishable from an empty install path.
fn throwaway_install(label: &str, version: &str) -> (PathBuf, PathBuf) {
    let (directory, path) = throwaway_binary(label, b"");
    write_install(&path, version);
    (directory, path)
}

/// Replace what is AT `path` with a binary reporting `version` — somebody
/// reinstalling out of band, which is the case the state row used to be unable
/// to notice.
fn write_install(path: &std::path::Path, version: &str) {
    std::fs::write(path, binary_reporting(version)).expect("writing the stand-in binary");
    make_executable(path);
}

#[cfg(unix)]
fn make_executable(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
        .expect("making the stand-in binary executable");
}

#[cfg(not(unix))]
fn make_executable(_path: &std::path::Path) {}

/// What the queue says TO the installation at `path` — the same scoped read a
/// daemon at that path performs when it builds an envelope.
async fn queue_for(pool: &fs3_store::PgPool, path: &std::path::Path) -> Vec<fs3_core::UserMessage> {
    fs3_store::live_messages(pool, &path.display().to_string())
        .await
        .expect("the queue")
}

#[tokio::test]
async fn a_newer_release_is_downloaded_verified_and_swapped_in() {
    let base = spawn_release_server("v9.9.9", published(&binary_reporting("9.9.9"))).await;
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
        binary_reporting("9.9.9")
    );
}

#[tokio::test]
async fn the_newest_release_already_running_is_a_no_op() {
    let base = spawn_release_server("v9.9.9", published(&binary_reporting("9.9.9"))).await;
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
    let base = spawn_release_server("v0.0.1", published(&binary_reporting("0.0.1"))).await;
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
    let mut assets = published(&binary_reporting("9.9.9"));
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
    let assets: Vec<(String, Vec<u8>)> = published(&binary_reporting("9.9.9"))
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
    let assets: Vec<(String, Vec<u8>)> = published(&binary_reporting("9.9.9"))
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
    let base = spawn_release_server("v9.9.9", published(&binary_reporting("9.9.9"))).await;

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

    let base = spawn_release_server("v9.9.9", published(&binary_reporting("9.9.9"))).await;
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
        std::fs::read(&installed).expect("reading back"),
        binary_reporting("9.9.9")
    );
}

/// The loop guard (req-0060). A published binary whose compiled-in version is
/// stale is permanently "older" than every release, so an updater that trusted
/// the tag alone would download and swap it once per interval FOREVER, raising
/// a restart message that restarting cannot clear.
///
/// Not hypothetical: v0.2.0 shipped reporting 0.1.0, because release-please
/// bumped its own manifest and not the workspace `Cargo.toml`.
#[cfg(unix)]
#[tokio::test]
async fn a_binary_that_lies_about_its_version_is_refused_rather_than_reinstalled_forever() {
    // The exact shape of the real defect: tagged 9.9.9, reports 0.1.0 — which
    // is also the version the updater is running.
    let base = spawn_release_server("v9.9.9", published(&binary_reporting("0.1.0"))).await;
    let (_directory, installed) = throwaway_binary("update-lying", b"the old binary");

    let outcome = Updater::against(&base, "0.1.0")
        .expect("building the updater")
        .at_path(installed.clone())
        .run_once()
        .await
        .expect("a lying binary is an outcome, not a failure");

    match outcome {
        Outcome::Blocked { latest, reason } => {
            assert_eq!(latest.to_string(), "9.9.9");
            assert!(reason.contains("reports itself as"), "reason: {reason}");
            assert!(
                reason.contains("reinstall it on every check forever"),
                "the reason must name the loop, which is WHY this is refused: {reason}"
            );
        }
        other => panic!("expected the swap to be refused, got {other:?}"),
    }
    assert_eq!(
        std::fs::read(&installed).expect("reading back"),
        b"the old binary",
        "the install path must be untouched — the whole point is that the swap never happened"
    );
}

/// The same guard catches the class a version comparison never could: an asset
/// that cannot execute at all — a build for the wrong triple, or a truncated
/// artefact whose checksum somehow matched.
#[cfg(unix)]
#[tokio::test]
async fn an_asset_that_cannot_be_executed_is_refused() {
    let base = spawn_release_server("v9.9.9", published(b"\x7fELF not really\n")).await;
    let (_directory, installed) = throwaway_binary("update-noexec", b"the old binary");

    let outcome = Updater::against(&base, "0.1.0")
        .expect("building the updater")
        .at_path(installed.clone())
        .run_once()
        .await
        .expect("an unrunnable asset is an outcome, not a failure");

    assert!(
        matches!(
            &outcome,
            Outcome::Blocked { reason, .. } if reason.contains("could not be run to confirm its version")
        ),
        "expected the probe to refuse it, got {outcome:?}"
    );
    assert_eq!(
        std::fs::read(&installed).expect("reading back"),
        b"the old binary"
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

    let base = spawn_release_server("v9.9.9", published(&binary_reporting("9.9.9"))).await;
    let (_directory, installed) = throwaway_install("update-reconcile", "0.1.0");

    let mut supervisor = UpdateSupervisor::new(pool.clone(), &UpdateConfig::default(), "0.1.0")
        .expect("building the supervisor")
        .against(&base, installed.clone())
        .expect("pointing it at the stub");

    let pass = supervisor.reconcile().await.expect("the first pass");
    assert_eq!(pass.changed, 1, "the first pass must run a check");
    assert_eq!(
        std::fs::read(&installed).expect("reading back"),
        binary_reporting("9.9.9")
    );

    let messages = queue_for(&pool, &installed).await;
    assert_eq!(messages.len(), 1, "expected one steer, got {messages:?}");
    assert_eq!(
        messages[0].key,
        format!("update:installed:9.9.9:{}", installed.display()),
        "the key names the install it is about — `key` is the queue's primary \
         key, so a shared one would be two installs overwriting each other"
    );
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
        queue_for(&pool, &installed).await.len(),
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

    let base = spawn_release_server("v9.9.9", published(&binary_reporting("9.9.9"))).await;
    let (_directory, installed) = throwaway_install("update-clear", "0.1.0");

    UpdateSupervisor::new(pool.clone(), &UpdateConfig::default(), "0.1.0")
        .expect("building the supervisor")
        .against(&base, installed.clone())
        .expect("pointing it at the stub")
        .reconcile()
        .await
        .expect("the installing pass");

    assert_eq!(queue_for(&pool, &installed).await.len(), 1);

    // The restarted daemon: same state row, new running version.
    let mut restarted = UpdateSupervisor::new(pool.clone(), &UpdateConfig::default(), "9.9.9")
        .expect("building the supervisor")
        .against(&base, installed.clone())
        .expect("pointing it at the stub");
    restarted.reconcile().await.expect("the post-restart pass");

    assert!(
        queue_for(&pool, &installed).await.is_empty(),
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

    let base = spawn_release_server("v9.9.9", published(&binary_reporting("9.9.9"))).await;
    let (_directory, installed) = throwaway_install("update-off", "0.1.0");

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
        binary_reporting("0.1.0"),
        "auto = false must never swap a binary"
    );
    assert!(queue_for(&pool, &installed).await.is_empty());

    database.destroy(pool).await;
}

/// Defect A, reproduced on Jordan's own production envelope on 2026-08-27: a
/// debug daemon run from a dev worktree the day before wrote `update:blocked`
/// naming its own `target/debug` path, and the production daemon restarted the
/// next morning on a current binary and went on carrying it.
///
/// The message was level-triggered all along; the level was only ever re-read
/// on the producer's 24h cadence, and BOOT DID NOT TICK IT. So the interval a
/// previous process had already claimed silently suppressed the one pass that
/// would have retracted the lie.
///
/// Two claims here, and both matter: a booting supervisor checks even though
/// the interval was claimed seconds ago, and the standing message that has
/// stopped being true is gone within that first pass.
#[tokio::test]
async fn a_booting_supervisor_checks_immediately_and_retracts_what_stopped_being_true() {
    let database = support::FreshDatabase::create("updateboot").await;
    let pool = database.pool().await;
    fs3_store::migrate(&pool).await.expect("migrations");

    // Nothing newer exists, so every pass here is a pure check: no download, no
    // swap, nothing but the truth about this install.
    let base = spawn_release_server("v0.1.0", published(&binary_reporting("0.1.0"))).await;
    let (_directory, installed) = throwaway_install("update-boot", "0.1.0");
    let scope = installed.display().to_string();

    let supervisor = || {
        UpdateSupervisor::new(pool.clone(), &UpdateConfig::default(), "0.1.0")
            .expect("building the supervisor")
            .against(&base, installed.clone())
            .expect("pointing it at the stub")
    };

    let mut running = supervisor();
    assert_eq!(
        running.reconcile().await.expect("the first pass").changed,
        1,
        "a fresh install must check rather than wait out an interval it never served"
    );
    assert_eq!(
        running.reconcile().await.expect("the second pass").changed,
        0,
        "the interval must still suppress a re-check within one process"
    );

    // A previous process left a standing message behind. This is the fossil,
    // in the shape the queue holds it.
    fs3_store::sync_messages(
        &pool,
        fs3_core::UPDATE_SOURCE,
        Some(&scope),
        &[fs3_core::UserMessage::new(
            format!("update:blocked:{scope}"),
            fs3_core::UPDATE_SOURCE,
            fs3_core::Severity::Warning,
            "update not possible: seeded by a process that is no longer running",
            "this is exactly what must not survive a restart",
        )],
    )
    .await
    .expect("seeding the fossil");
    assert_eq!(queue_for(&pool, &installed).await.len(), 1);

    // The daemon restarts. `last_checked_at` is seconds old and the interval is
    // 24h, so before this packet the boot pass did nothing at all.
    let mut rebooted = supervisor();
    assert_eq!(
        rebooted.reconcile().await.expect("the boot pass").changed,
        1,
        "every boot must re-evaluate, whatever the interval says"
    );
    assert!(
        queue_for(&pool, &installed).await.is_empty(),
        "a message whose cause is gone must not survive the boot that could see it"
    );

    database.destroy(pool).await;
}

/// Defect B (standing Linux tester, finding 12): update state was keyed to the
/// STORE, and an install is keyed to a PATH.
///
/// Not exotic — `install.sh` itself picks `/usr/local/bin` or `~/.local/bin`
/// depending on permissions, so one person who has ever installed both ways has
/// two installs against one database. They thrashed one row last-writer-wins,
/// and root's daemon ended up advertising another user's blocked update about a
/// path root does not use.
///
/// Both halves are proven: one install's message is invisible to the other, and
/// the other declaring its own state does not RETRACT it.
#[tokio::test]
async fn two_installs_sharing_a_store_neither_see_nor_retract_each_others_messages() {
    let database = support::FreshDatabase::create("updatepaths").await;
    let pool = database.pool().await;
    fs3_store::migrate(&pool).await.expect("migrations");

    let base = spawn_release_server("v9.9.9", published(&binary_reporting("9.9.9"))).await;
    // Root's install is stale and gets the new binary; alice's is already
    // current, and her daemon has nothing whatever to say.
    let (_root_dir, root) = throwaway_install("update-root", "0.1.0");
    let (_alice_dir, alice) = throwaway_install("update-alice", "9.9.9");

    UpdateSupervisor::new(pool.clone(), &UpdateConfig::default(), "0.1.0")
        .expect("building root's supervisor")
        .against(&base, root.clone())
        .expect("pointing it at the stub")
        .reconcile()
        .await
        .expect("root's pass");

    let at_root = queue_for(&pool, &root).await;
    assert_eq!(
        at_root.len(),
        1,
        "root is waiting on a restart: {at_root:?}"
    );
    assert!(at_root[0].text.contains(&root.display().to_string()));

    assert!(
        queue_for(&pool, &alice).await.is_empty(),
        "alice's install must not be told to restart a daemon that is not hers"
    );

    // Alice's daemon runs a full pass of its own. Under one shared row this is
    // the write that clobbered root; under per-source-only ownership it is the
    // declaration that retracted root's message.
    UpdateSupervisor::new(pool.clone(), &UpdateConfig::default(), "9.9.9")
        .expect("building alice's supervisor")
        .against(&base, alice.clone())
        .expect("pointing it at the stub")
        .reconcile()
        .await
        .expect("alice's pass");

    assert!(
        queue_for(&pool, &alice).await.is_empty(),
        "alice is current and must stay silent"
    );
    assert_eq!(
        queue_for(&pool, &root).await.len(),
        1,
        "one install declaring its own state must not retract another's"
    );

    database.destroy(pool).await;
}

/// Defect C (finding 12's tail): the row claimed 0.3.1 was installed at a path
/// that held 0.3.0, after a pinned reinstall at an older tag.
///
/// `record_installed` wrote what the updater DID and `record_clear` only ever
/// cleared the block, so nothing in the old code could unset `installed_version`
/// — the false "restart to pick up X" was permanent, and combined with no
/// rollback verb the only escape was hand-written SQL. Asking the file instead
/// makes the claim self-correcting: a swap and somebody else's reinstall are the
/// same question with the same answer.
#[tokio::test]
async fn an_out_of_band_change_at_the_install_path_corrects_the_claim_rather_than_outliving_it() {
    let database = support::FreshDatabase::create("updatedisk").await;
    let pool = database.pool().await;
    fs3_store::migrate(&pool).await.expect("migrations");

    // Nothing newer is published, so nothing here downloads or swaps: every
    // change to the install path is made behind the daemon's back, which is the
    // whole point.
    let base = spawn_release_server("v0.1.0", published(&binary_reporting("0.1.0"))).await;
    let (_directory, installed) = throwaway_install("update-disk", "0.3.1");

    let boot = || async {
        UpdateSupervisor::new(pool.clone(), &UpdateConfig::default(), "0.1.0")
            .expect("building the supervisor")
            .against(&base, installed.clone())
            .expect("pointing it at the stub")
            .reconcile()
            .await
            .expect("a boot pass");
    };

    boot().await;
    let waiting = queue_for(&pool, &installed).await;
    assert_eq!(waiting.len(), 1);
    assert_eq!(
        waiting[0].key,
        format!("update:installed:0.3.1:{}", installed.display())
    );

    // Somebody reinstalls at an older pinned tag.
    write_install(&installed, "0.3.0");
    boot().await;

    let corrected = queue_for(&pool, &installed).await;
    assert_eq!(
        corrected.len(),
        1,
        "still one message, not two: {corrected:?}"
    );
    assert_eq!(
        corrected[0].key,
        format!("update:installed:0.3.0:{}", installed.display()),
        "the claim must be rewritten to what is actually there"
    );
    assert!(
        corrected[0].text.contains("0.3.0") && !corrected[0].text.contains("0.3.1"),
        "the user must not be told to restart for a version no longer on disk: {}",
        corrected[0].text
    );

    // The install path comes to hold what this process is already running.
    write_install(&installed, "0.1.0");
    boot().await;
    assert!(
        queue_for(&pool, &installed).await.is_empty(),
        "matching disk and running version is the cleared condition"
    );

    // And the path stops holding anything at all. `None` is a real answer: a
    // binary somebody deleted cannot be restarted into.
    write_install(&installed, "0.3.2");
    boot().await;
    assert_eq!(queue_for(&pool, &installed).await.len(), 1);

    std::fs::remove_file(&installed).expect("removing the installed binary");
    boot().await;
    assert!(
        queue_for(&pool, &installed).await.is_empty(),
        "an install path that holds nothing must retract its restart steer"
    );

    database.destroy(pool).await;
}
