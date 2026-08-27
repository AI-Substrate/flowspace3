//! The real binary, the real log file (deliverables 1, 3 and 4, end to end).
//!
//! Everything else about logging is proved in `fs3-daemon`'s own tests against
//! the module directly. This one runs `flowspace3 daemon` as a user runs it,
//! because the thing that failed on 2026-08-27 was not a module — it was a
//! process nobody could get evidence out of afterwards.
//!
//! It needs no database. The daemon is pointed at a port nothing listens on,
//! so it boots, says where its log is, and then fails on the store — and every
//! assertion here is about what it wrote BEFORE that. Deliberately no
//! `fs3_testkit::database` gate: this test cannot reach a database, which is
//! the strongest form of "it will never write to the wrong one".

use std::process::Command;

/// A store address that is refused immediately rather than one that hangs:
/// port 1 is privileged, unbound, and answers `ECONNREFUSED` at once.
const NOTHING_LISTENING: &str = "postgres://nobody:nobody@127.0.0.1:1/none";

#[test]
fn the_daemon_writes_a_log_file_and_names_it_on_its_first_line() {
    let temporary = tempfile::tempdir().expect("a temp dir");
    let config_dir = temporary.path().join("config");
    let log_dir = temporary.path().join("logs");

    let run = Command::new(env!("CARGO_BIN_EXE_flowspace3"))
        .arg("daemon")
        .env("FS3_CONFIG_DIR", &config_dir)
        .env("FS3_DAEMON__LOG_DIR", &log_dir)
        .env("FS3_DATABASE__URL", NOTHING_LISTENING)
        // The config layer is what is under test; an inherited RUST_LOG would
        // silently take over the filter.
        .env_remove("RUST_LOG")
        .output()
        .expect("running the daemon");

    assert!(
        !run.status.success(),
        "the daemon should have failed on the store it cannot reach"
    );

    // The directory did not exist a moment ago: the daemon creates its own.
    let log_file = log_dir.join("flowspace3.log");
    let written = std::fs::read_to_string(&log_file)
        .unwrap_or_else(|error| panic!("{} was never written ({error})", log_file.display()));

    assert!(
        written.contains("fs3 daemon starting"),
        "the boot line must be in the FILE, not only on the terminal: {written}"
    );
    assert!(
        written.contains(&log_file.display().to_string()),
        "the boot line must name the log path, so 'where are the logs' is \
         answerable from the logs: {written}"
    );

    // Non-TTY on both destinations: `Command` gives the child a pipe, so this
    // is exactly the redirected-stdout case that used to fill a file with
    // escape sequences.
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(
        !stdout.contains('\u{1b}'),
        "stdout must drop colour when it is not a terminal: {stdout:?}"
    );
    assert!(
        !written.contains('\u{1b}'),
        "the log file must never carry escape sequences: {written:?}"
    );
}
