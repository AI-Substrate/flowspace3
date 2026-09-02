//! The boot contract: a daemon that cannot reach its store must die loudly.
//!
//! fs3's daemon is the single writer and the only migration point, so a store
//! it cannot reach means every request would fail. PRD req 37 says fail fast
//! and tell the user how to heal it — never start into a guaranteed error.
//!
//! This is the automated form of a check that used to be done by hand: run the
//! real binary against an unreachable database and read the exit code and the
//! stderr it produced.
//!
//! It lives in `fs3-cli` because since PRD req 51 that is where the binary is:
//! the daemon ships inside `flowspace3` as `flowspace3 daemon`. The contract it
//! defends is the daemon crate's; the artifact that has to honour it is this
//! one.

use std::io::Read;
use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};

/// The binary under test, built by cargo for this integration test.
const FLOWSPACE3: &str = env!("CARGO_BIN_EXE_flowspace3");

/// Port 1 is privileged and never serves Postgres, so the connection is
/// refused immediately rather than hanging.
const UNREACHABLE: &str = "postgres://fs3:hunter2@127.0.0.1:1/flowspace3";

/// Generous next to a refused TCP connection, short enough that a hang is a
/// test failure rather than a wedged CI job.
const PATIENCE: Duration = Duration::from_secs(60);

#[test]
fn a_daemon_that_cannot_reach_its_store_exits_non_zero_and_says_how_to_fix_it() {
    let dir = temp_dir("boot-contract");
    std::fs::write(
        dir.join("config.toml"),
        format!("[database]\nurl = \"{UNREACHABLE}\"\n"),
    )
    .expect("writing the fixture config");

    // `FromConfigFile`: the fixture written above IS the thing under test, so
    // pinning `FS3_DATABASE__URL` here would beat it and prove nothing — the
    // environment is the highest precedence layer (fs3_core::config). The seal
    // still scrubs every inherited `FS3_*`, and it refuses outright unless the
    // fixture really does set `[database].url`, which is what makes "this arm
    // sets no URL" safe rather than a hole.
    //
    // The scrub is not hypothetical: an ambient `FS3_DATABASE__URL` in a
    // developer's shell silently beat this fixture, the daemon reached a store
    // that IS running, served happily, and the test hung until its patience ran
    // out — reported as "the daemon did not fail fast". Observed live,
    // 2026-08-27.
    let mut child = fs3_testkit::sealed(
        Path::new(FLOWSPACE3),
        &dir,
        fs3_testkit::TestDatabase::FromConfigFile,
    )
    .arg("daemon")
    // Deterministic output regardless of the developer's own filter.
    .env("RUST_LOG", "fs3_daemon=info")
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .expect("the daemon binary should start");

    let status = wait_for(&mut child);

    let mut stderr = String::new();
    child
        .stderr
        .take()
        .expect("stderr was piped")
        .read_to_string(&mut stderr)
        .expect("reading the daemon's stderr");

    assert!(
        !status.success(),
        "the daemon must refuse to run without its store, got {status}. stderr:\n{stderr}"
    );

    // The message has to carry the two things that turn a failure into a fix:
    // which database was tried, and the command that starts it.
    assert!(
        stderr.contains("127.0.0.1:1/flowspace3"),
        "stderr must name the database that could not be reached:\n{stderr}"
    );
    assert!(
        stderr.contains("docker compose up -d"),
        "stderr must name the command that starts the store:\n{stderr}"
    );
    // ...and never the password that was in the URL it names.
    assert!(
        !stderr.contains("hunter2"),
        "the database password must never reach a log line:\n{stderr}"
    );

    std::fs::remove_dir_all(&dir).ok();
}

/// The blast-radius backstop, from the outside: a test that spawns this daemon
/// without choosing a database must not get a daemon.
///
/// This is the 2026-08-27 incident reconstructed exactly — marker env present
/// (a test run), no `[database]` anywhere, so the child would resolve
/// `DatabaseConfig::DEFAULT_URL` and MIGRATE it. On Jordan's machine that URL
/// is the production store; migration 0012 reached it this way.
///
/// The command is deliberately built by taking a sealed spawn and REMOVING the
/// pin. That is what makes this a real test of the backstop rather than of the
/// seal: the shape under test is the one `spawn_isolation.rs` forbids, and
/// starting from `sealed` is how it gets constructed without hand-rolling the
/// forbidden `Command::new` the scan would (correctly) reject.
#[test]
fn a_daemon_spawned_by_a_test_refuses_to_boot_against_a_defaulted_store() {
    let dir = temp_dir("defaulted-store");
    // No config.toml at all: `[database]` comes from nowhere, which is the
    // whole point.

    let mut command = fs3_testkit::sealed(
        Path::new(FLOWSPACE3),
        &dir,
        fs3_testkit::TestDatabase::Unreachable,
    );
    command
        .env_remove("FS3_DATABASE__URL")
        .env(fs3_testkit::TEST_DATABASE_ENV, UNREACHABLE)
        .env("RUST_LOG", "fs3_daemon=info");

    let mut child = command
        .arg("daemon")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the daemon binary should start");

    let status = wait_for(&mut child);

    let mut stderr = String::new();
    child
        .stderr
        .take()
        .expect("stderr was piped")
        .read_to_string(&mut stderr)
        .expect("reading the daemon's stderr");

    assert!(
        !status.success(),
        "a daemon spawned by a test with no database chosen must refuse to boot, \
         because booting means MIGRATING whatever it defaulted to:\n{stderr}"
    );
    assert!(
        stderr.contains("refusing to boot"),
        "the refusal has to say it is refusing:\n{stderr}"
    );
    assert!(
        stderr.contains(fs3_testkit::TEST_DATABASE_ENV),
        "the refusal must name the marker that identified this as a test run, so \
         the reader can tell it apart from a real misconfiguration:\n{stderr}"
    );
    assert!(
        stderr.contains("fs3_testkit::sealed"),
        "a refusal that does not name the fix is a puzzle:\n{stderr}"
    );

    std::fs::remove_dir_all(&dir).ok();
}

/// Kills a spawned daemon however the test exits, including an assertion panic.
struct Daemon(std::process::Child);

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Regression for the shared-key outage: a hostname can resolve to both
/// loopback families, and Tokio's string bind falls through to the other family
/// when the first is occupied. Both processes then believe they own one
/// configured endpoint and the loser publishes over the winner's key.
#[tokio::test]
async fn a_second_daemon_cannot_publish_via_another_loopback_address_family() {
    assert_second_daemon_loses_without_touching_the_key(false).await;
}

#[tokio::test]
async fn a_second_daemon_with_json_cannot_publish_via_another_loopback_address_family() {
    assert_second_daemon_loses_without_touching_the_key(true).await;
}

#[tokio::test]
async fn cli_names_a_shared_key_overwrite_and_the_listener_owner() {
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .expect("reserving an ephemeral port")
        .local_addr()
        .expect("reading the ephemeral port")
        .port();
    let database =
        fs3_testkit::FreshDatabase::create_from(&fs3_testkit::test_database_url(), "key-hint")
            .await
            .expect("creating a per-run database on the test postmaster");
    let config = tempfile::tempdir().expect("an isolated config directory");
    std::fs::write(
        config.path().join("config.toml"),
        format!(
            "[daemon]\nurl = \"http://127.0.0.1:{port}\"\nlog_dir = {:?}\n\n\
             [database]\nurl = {:?}\n\n\
             [embedder]\nactive = \"fake\"\n\n\
             [summarizer]\nactive = \"fake\"\n",
            config.path().join("logs").display().to_string(),
            database.url()
        ),
    )
    .expect("writing isolated daemon configuration");

    let mut daemon = spawn_daemon(config.path(), false);
    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .expect("building a loopback-only client");
    let base = format!("http://127.0.0.1:{port}");
    let key_path = fs3_core::daemon_key_path(config.path());
    let _original_key = wait_until_authorized(&mut daemon, &client, &base, &key_path).await;
    let original_mtime = std::fs::metadata(&key_path)
        .expect("published key metadata")
        .modified()
        .expect("published key mtime");
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        tokio::time::sleep(Duration::from_millis(10)).await;
        std::fs::write(&key_path, "replacement-key").expect("simulating a shared key overwrite");
        if std::fs::metadata(&key_path)
            .expect("replacement key metadata")
            .modified()
            .expect("replacement key mtime")
            > original_mtime
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the fixture filesystem never advanced daemon.key's mtime"
        );
    }

    let output = fs3_testkit::sealed(
        Path::new(FLOWSPACE3),
        config.path(),
        fs3_testkit::TestDatabase::FromConfigFile,
    )
    .current_dir(config.path())
    .arg("ping")
    .output()
    .expect("running the real CLI against the original daemon");
    let rendered = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let _ = daemon.0.kill();
    let _ = daemon.0.wait();
    database
        .cleanup()
        .await
        .expect("dropping the per-run test database");

    assert!(
        !output.status.success(),
        "the overwritten key must be rejected"
    );
    assert!(
        rendered.contains("another flowspace3 daemon overwrote the shared key"),
        "CLI did not explain the overwrite:\n{rendered}"
    );
    assert!(
        rendered.contains(&format!("restart the daemon that owns :{port}")),
        "CLI did not name the listener owner:\n{rendered}"
    );
}

async fn assert_second_daemon_loses_without_touching_the_key(json: bool) {
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .expect("reserving an ephemeral port")
        .local_addr()
        .expect("reading the ephemeral port")
        .port();
    let database =
        fs3_testkit::FreshDatabase::create_from(&fs3_testkit::test_database_url(), "daemon-key")
            .await
            .expect("creating a per-run database on the test postmaster");
    let database_url = database.url();
    let config = tempfile::tempdir().expect("an isolated config directory");
    let log_dir = config.path().join("logs");
    std::fs::write(
        config.path().join("config.toml"),
        format!(
            "[daemon]\nurl = \"http://localhost:{port}\"\nlog_dir = {:?}\n\n\
             [database]\nurl = {:?}\n\n\
             [embedder]\nactive = \"fake\"\n\n\
             [summarizer]\nactive = \"fake\"\n",
            log_dir.display().to_string(),
            database_url
        ),
    )
    .expect("writing isolated daemon configuration");

    let mut first = spawn_daemon(config.path(), false);
    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .expect("building a loopback-only client");
    let base = format!("http://localhost:{port}");
    let key_path = fs3_core::daemon_key_path(config.path());
    let original_key = wait_until_authorized(&mut first, &client, &base, &key_path).await;
    let original_mtime = std::fs::metadata(&key_path)
        .expect("the serving daemon published its key")
        .modified()
        .expect("daemon.key has an mtime");
    let files_before = regular_files(config.path());

    // Keep mtime part of the witness rather than relying only on random bytes.
    tokio::time::sleep(Duration::from_millis(20)).await;
    let mut second = spawn_daemon(config.path(), json);
    let deadline = Instant::now() + PATIENCE;
    let status = loop {
        if let Some(status) = second.0.try_wait().expect("polling the second daemon") {
            break Some(status);
        }
        if std::fs::read_to_string(&key_path).is_ok_and(|key| key != original_key) {
            break None;
        }
        assert!(
            Instant::now() < deadline,
            "the second daemon neither failed its bind nor published within {PATIENCE:?}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    };

    if status.is_none() {
        let replacement_key =
            std::fs::read_to_string(&key_path).expect("the second daemon replaced daemon.key");
        let v4_old = authorized(&client, &format!("http://127.0.0.1:{port}"), &original_key).await;
        let v4_new = authorized(
            &client,
            &format!("http://127.0.0.1:{port}"),
            &replacement_key,
        )
        .await;
        let v6_old = authorized(&client, &format!("http://[::1]:{port}"), &original_key).await;
        let v6_new = authorized(&client, &format!("http://[::1]:{port}"), &replacement_key).await;
        let _ = second.0.kill();
        let _ = second.0.wait();
        let _ = first.0.kill();
        let _ = first.0.wait();
        database
            .cleanup()
            .await
            .expect("dropping the per-run test database after the red witness");
        panic!(
            "{} was rewritten by daemon B: TcpListener::bind(\"localhost:{port}\") fell through \
             from daemon A's occupied address family to the other loopback family, then reached \
             StagedAuth::publish (v4 old/new={v4_old}/{v4_new}, v6 old/new={v6_old}/{v6_new}, \
             --json={json})",
            key_path.display()
        );
    }

    let status = status.expect("the completed second daemon has a status");
    assert!(!status.success(), "the bind loser must exit non-zero");
    assert_eq!(
        std::fs::read_to_string(&key_path).expect("reading the winner's key"),
        original_key,
        "a bind loser must not change daemon.key bytes (--json={json})"
    );
    assert_eq!(
        std::fs::metadata(&key_path)
            .expect("reading winner key metadata")
            .modified()
            .expect("daemon.key has an mtime"),
        original_mtime,
        "a bind loser must not change daemon.key mtime (--json={json})"
    );
    assert_eq!(
        regular_files(config.path()),
        files_before,
        "a failed bind must not leave a staged key file (--json={json})"
    );
    assert!(
        authorized(&client, &base, &original_key).await,
        "daemon A's clients must remain authorized (--json={json})"
    );
    let _ = first.0.kill();
    let _ = first.0.wait();
    database
        .cleanup()
        .await
        .expect("dropping the per-run test database");
}

fn spawn_daemon(config_dir: &Path, json: bool) -> Daemon {
    let mut command = fs3_testkit::sealed(
        Path::new(FLOWSPACE3),
        config_dir,
        fs3_testkit::TestDatabase::FromConfigFile,
    );
    command
        .current_dir(config_dir)
        .arg("daemon")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if json {
        command.arg("--json");
    }
    Daemon(
        command
            .spawn()
            .expect("the real daemon binary should start"),
    )
}

async fn wait_until_authorized(
    daemon: &mut Daemon,
    client: &reqwest::Client,
    base: &str,
    key_path: &Path,
) -> String {
    let deadline = Instant::now() + PATIENCE;
    loop {
        if let Some(status) = daemon.0.try_wait().expect("polling the first daemon") {
            let (stdout, stderr) = child_output(&mut daemon.0);
            panic!("daemon A exited before serving {base}: {status}\n{stdout}{stderr}");
        }
        if let Ok(key) = std::fs::read_to_string(key_path)
            && authorized(client, base, &key).await
        {
            return key;
        }
        assert!(
            Instant::now() < deadline,
            "daemon A never served {base} within {PATIENCE:?}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn child_output(child: &mut std::process::Child) -> (String, String) {
    let mut stdout = String::new();
    if let Some(mut pipe) = child.stdout.take() {
        pipe.read_to_string(&mut stdout)
            .expect("reading daemon stdout");
    }
    let mut stderr = String::new();
    if let Some(mut pipe) = child.stderr.take() {
        pipe.read_to_string(&mut stderr)
            .expect("reading daemon stderr");
    }
    (stdout, stderr)
}

async fn authorized(client: &reqwest::Client, base: &str, key: &str) -> bool {
    client
        .get(format!("{base}/health"))
        .bearer_auth(key.trim())
        .send()
        .await
        .is_ok_and(|response| response.status().is_success())
}

fn regular_files(directory: &Path) -> std::collections::BTreeSet<std::ffi::OsString> {
    std::fs::read_dir(directory)
        .expect("reading the config directory")
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .map(|entry| entry.file_name())
        .collect()
}

/// Wait for the child, killing it if it somehow decided to keep running.
fn wait_for(child: &mut std::process::Child) -> std::process::ExitStatus {
    let deadline = Instant::now() + PATIENCE;
    loop {
        match child.try_wait().expect("polling the daemon process") {
            Some(status) => return status,
            None if Instant::now() >= deadline => {
                child.kill().ok();
                child.wait().ok();
                panic!(
                    "the daemon was still running after {PATIENCE:?} with an unreachable store: \
                     it should have failed fast (PRD req 37)"
                );
            }
            None => std::thread::sleep(Duration::from_millis(50)),
        }
    }
}

/// A fresh directory under the system temp dir, unique per call.
fn temp_dir(label: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);

    let path = std::env::temp_dir().join(format!(
        "fs3-{label}-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&path).expect("creating a temp config directory");
    path
}
