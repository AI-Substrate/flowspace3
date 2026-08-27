//! A log destination that cannot be written degrades; it never crashes.
//!
//! Its own test binary for the same reason as `logging_file.rs`: installing a
//! subscriber is a once-per-process act, and this test needs the FAILING
//! install rather than the successful one.
//!
//! The invariant under test is the one that matters at 3am: the daemon's job
//! is indexing, and a log file it cannot open must never be the reason it
//! refuses to serve. What it must do instead is SAY so — on stdout, and
//! through the user-messages queue.

use fs3_core::DaemonConfig;

#[test]
fn an_unwritable_log_directory_degrades_to_stdout_and_raises_a_message() {
    let directory = tempfile::tempdir().expect("a temp dir");

    // A FILE where the log directory should be. `create_dir_all` cannot make a
    // directory under it, which is a real failure mode (a stray file, a path
    // typo) rather than a permissions trick that needs root to arrange or
    // behaves differently on CI.
    let blocker = directory.path().join("not-a-directory");
    std::fs::write(&blocker, b"in the way").expect("writing the blocker");
    let configuration = DaemonConfig {
        log_dir: blocker.join("logs").display().to_string(),
        ..DaemonConfig::default()
    };

    let logging = fs3_daemon::logging::init(&configuration);

    assert!(
        logging.file.is_none(),
        "there is no file to write: {logging:?}"
    );
    let problem = logging
        .problem
        .as_deref()
        .expect("a reason, in words a user can act on");

    // Logging still works — this call would panic through the subscriber's
    // absence if `init` had bailed out instead of falling back to stdout.
    tracing::info!("the daemon carries on without a file");

    let messages = logging.desired_messages();
    assert_eq!(messages.len(), 1, "exactly one message: {messages:?}");
    assert_eq!(messages[0].source, fs3_core::LOGGING_SOURCE);
    assert!(
        messages[0].text.contains(problem),
        "the message must carry the reason, not just the fact: {:?}",
        messages[0]
    );
    assert!(
        !messages[0].next_action.is_empty(),
        "req-0059: a message a user cannot act on is a log line"
    );
}
