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
