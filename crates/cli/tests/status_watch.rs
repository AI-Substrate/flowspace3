//! `status --watch` is the one stdout path that is NDJSON, not an envelope.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

const TEST_KEY: &str = "isolated-watch-test-key";
const STREAM: &str = concat!(
    "{\"stream\":\"fs3.events\",\"v\":1,\"daemon\":\"0.4.0\",\"heartbeat_ms\":15000}\n",
    "{\"v\":1,\"at\":\"2026-08-28T03:11:05.001Z\",\"kind\":\"job_done\",\"job\":\"scan_file\",\"subject\":\"src/lib.rs\",\"ms\":12,\"left\":0}\n",
);
const INCOMPATIBLE_STREAM: &str = concat!(
    "{\"stream\":\"fs3.events\",\"v\":2,\"daemon\":\"9.0.0\",\"heartbeat_ms\":15000}\n",
    "{\"v\":2,\"at\":\"2026-08-28T03:11:05.001Z\",\"kind\":\"job_done\",\"job\":\"scan_file\",\"subject\":\"src/lib.rs\",\"ms\":12,\"left\":0}\n",
);
const DRIFTED_EVENT_STREAM: &str = concat!(
    "{\"stream\":\"fs3.events\",\"v\":1,\"daemon\":\"0.4.0\",\"heartbeat_ms\":15000}\n",
    "{\"v\":2,\"at\":\"2026-08-28T03:11:05.001Z\",\"kind\":\"job_done\",\"job\":\"scan_file\",\"subject\":\"src/lib.rs\",\"ms\":12,\"left\":0}\n",
);

fn flowspace3(config_dir: &Path) -> Command {
    let key_path = fs3_core::daemon_key_path(config_dir);
    std::fs::write(&key_path, TEST_KEY).expect("writes the isolated daemon key");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600))
            .expect("restricts the isolated daemon key");
    }
    fs3_testkit::sealed(
        Path::new(env!("CARGO_BIN_EXE_flowspace3")),
        config_dir,
        fs3_testkit::TestDatabase::Unreachable,
    )
}

/// Serve exactly one finite stream, then close. EOF is the watch's test exit.
fn serve_once(body: &'static str) -> (String, std::thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("binds an ephemeral port");
    let address = listener.local_addr().expect("bound address");
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accepts the watcher");
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("sets request deadline");
        let mut request = [0_u8; 4096];
        let read = stream.read(&mut request).expect("reads request");
        let request = String::from_utf8_lossy(&request[..read]).to_string();
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/x-ndjson\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream
            .write_all(response.as_bytes())
            .expect("writes stream");
        stream.flush().expect("flushes stream");
        request
    });
    (format!("http://{address}"), server)
}

/// Wait at most five seconds; an endless test is a defect in the test harness.
fn output_with_deadline(mut command: Command) -> Output {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn().expect("starts flowspace3");
    let deadline = Instant::now() + Duration::from_secs(5);
    let status = loop {
        if let Some(status) = child.try_wait().expect("checks child") {
            break status;
        }
        if Instant::now() >= deadline {
            child.kill().ok();
            child.wait().ok();
            panic!("status --watch did not finish after the finite server closed");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    child
        .stdout
        .take()
        .expect("stdout pipe")
        .read_to_end(&mut stdout)
        .expect("reads stdout");
    child
        .stderr
        .take()
        .expect("stderr pipe")
        .read_to_end(&mut stderr)
        .expect("reads stderr");
    Output {
        status,
        stdout,
        stderr,
    }
}

#[test]
fn json_watch_copies_daemon_ndjson_byte_for_byte() {
    let (url, server) = serve_once(STREAM);
    let config = tempfile::tempdir().expect("temporary config");
    let mut command = flowspace3(config.path());
    command.args(["status", "--watch", "--json", "--daemon-url", &url]);
    let output = output_with_deadline(command);
    let request = server.join().expect("server finishes");

    assert!(
        request
            .to_ascii_lowercase()
            .contains(&format!("authorization: bearer {TEST_KEY}"))
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, STREAM.as_bytes());
    assert!(output.stderr.is_empty());
}

#[test]
fn human_watch_is_a_terse_feed_not_an_envelope() {
    let (url, server) = serve_once(STREAM);
    let config = tempfile::tempdir().expect("temporary config");
    let mut command = flowspace3(config.path());
    command.args(["status", "--watch", "--human", "--daemon-url", &url]);
    let output = output_with_deadline(command);
    server.join().expect("server finishes");
    let stdout = String::from_utf8(output.stdout).expect("utf-8");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.starts_with("watching fs3.events v1"), "{stdout}");
    assert!(
        stdout.contains("done scan_file src/lib.rs (12ms, 0 left)"),
        "{stdout}"
    );
    assert!(
        !stdout.contains("\"ok\""),
        "watch is never envelope-shaped: {stdout}"
    );
}

#[test]
fn incompatible_version_is_rejected_for_humans_but_raw_json_is_untouched() {
    for (mode, succeeds) in [("--human", false), ("--json", true)] {
        let (url, server) = serve_once(INCOMPATIBLE_STREAM);
        let config = tempfile::tempdir().expect("temporary config");
        let mut command = flowspace3(config.path());
        command.args(["status", "--watch", mode, "--daemon-url", &url]);
        let output = output_with_deadline(command);
        server.join().expect("server finishes");

        assert_eq!(output.status.success(), succeeds, "mode {mode}");
        if succeeds {
            assert_eq!(output.stdout, INCOMPATIBLE_STREAM.as_bytes());
            assert!(output.stderr.is_empty());
        } else {
            let stderr = String::from_utf8(output.stderr).expect("utf-8 stderr");
            assert!(
                stderr.contains("unsupported event stream version v2"),
                "{stderr}"
            );
        }
    }
}

#[test]
fn a_version_drift_after_a_compatible_hello_is_rejected() {
    let (url, server) = serve_once(DRIFTED_EVENT_STREAM);
    let config = tempfile::tempdir().expect("temporary config");
    let mut command = flowspace3(config.path());
    command.args(["status", "--watch", "--human", "--daemon-url", &url]);
    let output = output_with_deadline(command);
    server.join().expect("server finishes");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("utf-8 stderr");
    assert!(
        stderr.contains("unsupported event line version v2"),
        "{stderr}"
    );
}
