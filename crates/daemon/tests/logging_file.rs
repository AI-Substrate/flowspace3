//! The log FILE, proved: panics land in it and no escape sequence ever does.
//!
//! One test binary, one process, one global subscriber — which is why this
//! file holds a single test rather than several. Installing a subscriber is a
//! process-wide, once-only act, so a second `#[test]` here would either race
//! this one or silently assert against the first one's installation.
//!
//! This is the packet's central proof. On 2026-08-27 the summarize lane
//! panicked inside a spawned task and the only copy of the evidence was a
//! terminal's scrollback; the assertion below is that exact shape — a panic in
//! a `tokio::spawn`ed task, read back off the disk afterwards.

use std::fs;

use fs3_core::DaemonConfig;

#[test]
fn a_panic_in_a_spawned_task_lands_in_the_log_file_free_of_escape_sequences() {
    let directory = tempfile::tempdir().expect("a temp dir");
    let configuration = DaemonConfig {
        log_dir: directory.path().display().to_string(),
        // Everything, so nothing filtered explains an absent line.
        log_level: "trace".to_string(),
        ..DaemonConfig::default()
    };

    let logging = fs3_daemon::logging::init(&configuration);
    assert_eq!(
        logging.problem, None,
        "a writable temp dir must produce a log file"
    );
    let path = logging.file.clone().expect("an active log file");

    tracing::info!(marker = "boot", "the daemon would say this at startup");

    // The incident, reproduced: a panic inside a spawned task. Nothing in the
    // task's own code observes it — the panic hook installed by `init` is the
    // only thing that can, which is why this is the test that would have
    // saved the evening of 2026-08-27.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("a runtime");
    let joined = runtime.block_on(async {
        tokio::spawn(async {
            panic!("the summarize lane fell over");
        })
        .await
    });
    assert!(joined.is_err(), "the task must have panicked");

    let written = fs::read_to_string(&path).expect("reading the log file");

    assert!(
        written.contains("the daemon would say this at startup"),
        "ordinary events must reach the file: {written}"
    );
    assert!(
        written.contains("the summarize lane fell over"),
        "the panic payload must reach the file: {written}"
    );
    assert!(
        written.contains("logging_file.rs"),
        "the panic's location must reach the file: {written}"
    );

    // Assert ABSENCE rather than stripping and comparing: a test that strips
    // escapes first would pass on a file nobody can read (the standing Linux
    // tester's lesson from the redirected-stdout finding).
    assert!(
        !written.contains('\u{1b}'),
        "the file layer must never emit ANSI: {written:?}"
    );
}
