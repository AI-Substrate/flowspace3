//! `flowspace3 ping`, end to end through the real binary.
//!
//! Both directions matter. The healthy path proves the client speaks the
//! daemon's health shape; the unreachable path proves PRD req 37 — fail fast,
//! name `flowspace3 doctor`, and never start infrastructure behind the user's
//! back.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::Command;

const TEST_KEY: &str = "isolated-cli-test-key";

/// A `flowspace3` sealed against the production store.
///
/// Every test here passes `--daemon-url` and talks to a hand-rolled stub, so
/// none of them opens a pool. That is not a reason to leave the environment
/// alone: unsealed, these spawns read the developer's real `config.toml` AND
/// their real `secrets.env`, and they are one change to a startup path away
/// from being the 2026-08-27 incident again (see `fs3_testkit::spawn`).
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

/// A one-shot HTTP server that answers a single request with `body`.
///
/// Hand-rolled rather than pulled in as a dependency: the CLI's contract with
/// the daemon is "GET /health returns this JSON", and 20 lines of socket code
/// states exactly that with nothing in between.
fn serve_once(body: &'static str) -> (String, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("an ephemeral port");
    let address = listener.local_addr().expect("the socket is bound");

    let handle = std::thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        let mut request = [0_u8; 1024];
        let _ = stream.read(&mut request);
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.flush();
    });

    (format!("http://{address}"), handle)
}

/// Bind a port, then release it — a URL that is guaranteed to refuse.
fn unused_url() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("an ephemeral port");
    let address = listener.local_addr().expect("the socket is bound");
    drop(listener);
    format!("http://{address}")
}

#[test]
fn ping_reports_a_healthy_daemon() {
    let (url, server) =
        serve_once(r#"{"status":"ok","version":"0.1.0","embedder":"fake","summarizer":"fake"}"#);

    let config = tempfile::tempdir().expect("a temp config directory");
    let output = flowspace3(config.path())
        .args(["ping", "--daemon-url", &url])
        .output()
        .expect("the flowspace3 binary should run");

    server.join().ok();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "expected success, got {:?}\nstdout: {stdout}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("healthy"), "stdout was: {stdout}");
    assert!(
        stdout.contains("fake"),
        "the arms in use should be visible: {stdout}"
    );
}

/// PRD req 37: fail fast, and say what to run.
#[test]
fn ping_without_a_daemon_exits_non_zero_and_suggests_doctor() {
    let url = unused_url();

    let config = tempfile::tempdir().expect("a temp config directory");
    let output = flowspace3(config.path())
        .args(["ping", "--daemon-url", &url])
        .output()
        .expect("the flowspace3 binary should run");

    assert!(
        !output.status.success(),
        "an unreachable daemon must be a non-zero exit"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not reachable"), "stderr was: {stderr}");
    assert!(
        stderr.contains(&url),
        "the failure should name the URL it tried: {stderr}"
    );
    assert!(
        stderr.contains("flowspace3 doctor"),
        "PRD req 37 requires the doctor suggestion: {stderr}"
    );
}

/// A missing key fails before network I/O and names the exact recovery.
#[test]
fn ping_without_a_key_is_actionable() {
    let config = tempfile::tempdir().expect("a temp config directory");
    let output = fs3_testkit::sealed(
        Path::new(env!("CARGO_BIN_EXE_flowspace3")),
        config.path(),
        fs3_testkit::TestDatabase::Unreachable,
    )
    .args(["ping", "--daemon-url", "http://127.0.0.1:1"])
    .output()
    .expect("the flowspace3 binary should run");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("daemon.key"),
        "the missing file is named: {stderr}"
    );
    assert!(
        stderr.contains("restart"),
        "the recovery is named: {stderr}"
    );
}

/// A daemon that answers but is unwell is not a healthy daemon.
#[test]
fn ping_refuses_a_daemon_that_reports_a_bad_status() {
    let (url, server) = serve_once(r#"{"status":"degraded"}"#);

    let config = tempfile::tempdir().expect("a temp config directory");
    let output = flowspace3(config.path())
        .args(["ping", "--daemon-url", &url])
        .output()
        .expect("the flowspace3 binary should run");

    server.join().ok();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("degraded"), "stderr was: {stderr}");
}

/// The client is a plain HTTP client — no keep-alive assumptions, no retries.
#[test]
fn the_client_sends_a_get_to_health() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("an ephemeral port");
    let address = listener.local_addr().expect("the socket is bound");

    let probe = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("the CLI should connect");
        let mut buffer = [0_u8; 1024];
        let read = stream.read(&mut buffer).unwrap_or(0);
        let request = String::from_utf8_lossy(&buffer[..read]).to_string();
        let _ = stream.write_all(
            b"HTTP/1.1 200 OK\r\nContent-Length: 15\r\nConnection: close\r\n\r\n{\"status\":\"ok\"}",
        );
        request
    });

    let config = tempfile::tempdir().expect("a temp config directory");
    let _ = flowspace3(config.path())
        .args(["ping", "--daemon-url", &format!("http://{address}")])
        .output();

    let request = probe.join().expect("the probe thread should finish");
    assert!(
        request.starts_with("GET /health "),
        "request was: {request}"
    );
    assert!(
        request
            .lines()
            .any(|line| line.eq_ignore_ascii_case(&format!("authorization: Bearer {TEST_KEY}"))),
        "every daemon request must carry the config directory's key: {request}"
    );
}

/// A trailing slash in configuration must not become `//health`.
#[test]
fn a_trailing_slash_in_the_url_is_tolerated() {
    let (url, server) = serve_once(r#"{"status":"ok","version":"0.1.0"}"#);

    let config = tempfile::tempdir().expect("a temp config directory");
    let output = flowspace3(config.path())
        .args(["ping", "--daemon-url", &format!("{url}/")])
        .output()
        .expect("the flowspace3 binary should run");

    server.join().ok();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
