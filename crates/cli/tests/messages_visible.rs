//! Standing conditions survive the human layer.
//!
//! # Why this file exists
//!
//! PRD req 59 messages are not part of any command's ANSWER — they are
//! standing conditions of the installation, attached to every envelope the
//! daemon serves ("a newer binary is waiting for a restart", "the schema is
//! ahead of this build"). An agent reads them in the envelope; a person reads
//! them on stderr.
//!
//! The human layer nearly ate them. Suppressing the duplicated failure render
//! in human mode also suppressed the messages loop beside it, and because no
//! render surface draws messages, they vanished entirely — exit 0, nothing on
//! stdout, nothing on stderr. Found in review on 2026-08-28, before it shipped.
//!
//! One test, one property: whichever way the answer is dressed, a standing
//! message is shown exactly once.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::Command;

const TEST_KEY: &str = "isolated-messages-test-key";

fn flowspace3(config_dir: &Path) -> Command {
    let key_path = fs3_core::daemon_key_path(config_dir);
    std::fs::write(&key_path, TEST_KEY).expect("writing the isolated daemon key");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600))
            .expect("restricting the isolated daemon key");
    }
    fs3_testkit::sealed(
        Path::new(env!("CARGO_BIN_EXE_flowspace3")),
        config_dir,
        fs3_testkit::TestDatabase::Unreachable,
    )
}

/// Answer one request with `body`, then stop; never block the suite.
fn serve(body: Vec<u8>) -> (String, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("an ephemeral port");
    let address = listener.local_addr().expect("the socket is bound");
    listener
        .set_nonblocking(true)
        .expect("the listener accepts a nonblocking mode");

    let handle = std::thread::spawn(move || {
        let started = std::time::Instant::now();
        loop {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    stream.set_nonblocking(false).ok();
                    let mut request = [0_u8; 4096];
                    let _ = stream.read(&mut request);
                    let head = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = stream.write_all(head.as_bytes());
                    let _ = stream.write_all(&body);
                    let _ = stream.flush();
                    return;
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    if started.elapsed() > std::time::Duration::from_secs(20) {
                        return;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                Err(_) => return,
            }
        }
    });

    (format!("http://{address}"), handle)
}

/// The frozen answer used here is the goldens' own `messages.json`: a status
/// envelope carrying one `warning` about a restart being due.
fn status_with_a_standing_message(mode: &str) -> (String, String) {
    let body = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/goldens/responses/messages.json"),
    )
    .expect("the frozen answer is readable");
    let (url, server) = serve(body);

    let config = tempfile::tempdir().expect("a temp config directory");
    let mut command = flowspace3(config.path());
    command.args(["status", "--daemon-url", &url, mode]);
    let output = command.output().expect("the binary runs");
    server.join().ok();

    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// The message is on stderr in JSON mode — and in the envelope on stdout.
#[test]
fn a_standing_message_reaches_a_reader_in_json_mode() {
    let (stdout, stderr) = status_with_a_standing_message("--json");

    assert!(
        stdout.contains("update.restart-pending"),
        "the envelope carries the message for an agent: {stdout}"
    );
    assert_eq!(
        stderr.matches("waiting for a daemon restart").count(),
        1,
        "a person reading the terminal sees it once: {stderr}"
    );
}

/// And it must still reach a reader when the answer is a rendered screen.
#[test]
fn a_standing_message_survives_the_human_layer() {
    let (stdout, stderr) = status_with_a_standing_message("--human");

    let shown = stdout.matches("waiting for a daemon restart").count()
        + stderr.matches("waiting for a daemon restart").count();

    assert_eq!(
        shown, 1,
        "a standing condition must be shown exactly once in human mode — not \
         swallowed, not doubled.\nstdout: {stdout}\nstderr: {stderr}"
    );
}
