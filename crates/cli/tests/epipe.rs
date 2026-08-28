//! Broken-pipe behavior at the CLI's shared output seam.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::{Command, Stdio};

const TEST_KEY: &str = "isolated-cli-test-key";

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

fn serve_once(body: String) -> (String, std::thread::JoinHandle<()>) {
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

#[test]
fn reader_closing_after_one_byte_is_a_quiet_success() {
    let body = serde_json::json!({
        "ok": true,
        "command": "status",
        "v": 1,
        "data": { "padding": "x".repeat(8 * 1024 * 1024) }
    })
    .to_string();
    let (url, server) = serve_once(body);
    let config = tempfile::tempdir().expect("a temp config directory");
    let mut child = flowspace3(config.path())
        .args(["status", "--daemon-url", &url])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the flowspace3 binary should start");

    let mut stdout = child.stdout.take().expect("stdout is piped");
    let mut first_byte = [0_u8; 1];
    stdout
        .read_exact(&mut first_byte)
        .expect("the command should emit one byte");
    drop(stdout);

    let output = child.wait_with_output().expect("the command should exit");
    server.join().expect("the stub server should exit");
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "expected quiet success, got {:?}; stderr: {stderr}",
        output.status
    );
    assert!(!stderr.contains("panicked at"), "stderr: {stderr}");
    assert!(!stderr.contains("Broken pipe"), "stderr: {stderr}");
}

#[test]
fn stderr_reader_closing_after_one_byte_is_a_quiet_success() {
    let body = serde_json::json!({
        "ok": false,
        "command": "status",
        "v": 1,
        "error": {
            "code": "FS3-E-TEST",
            "message": "x".repeat(8 * 1024 * 1024),
            "fix": "retry the test",
            "retryable": false
        }
    })
    .to_string();
    let (url, server) = serve_once(body);
    let config = tempfile::tempdir().expect("a temp config directory");
    let mut child = flowspace3(config.path())
        .args(["status", "--daemon-url", &url])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the flowspace3 binary should start");

    let mut stderr = child.stderr.take().expect("stderr is piped");
    let mut first_byte = [0_u8; 1];
    stderr
        .read_exact(&mut first_byte)
        .expect("the command should emit one error byte");
    drop(stderr);

    let output = child.wait_with_output().expect("the command should exit");
    server.join().expect("the stub server should exit");

    assert!(
        output.status.success(),
        "expected quiet success, got {:?}",
        output.status
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn successful_envelope_bytes_are_unchanged() {
    let body = r#"{"ok":true,"command":"status","v":1,"data":{"roots":[]}}"#;
    let expected = format!(
        "{}\n",
        serde_json::to_string_pretty(
            &serde_json::from_str::<fs3_core::envelope::Envelope>(body)
                .expect("the fixture is an envelope")
        )
        .expect("the envelope serializes")
    );
    let (url, server) = serve_once(body.to_string());
    let config = tempfile::tempdir().expect("a temp config directory");
    let output = flowspace3(config.path())
        .args(["status", "--daemon-url", &url])
        .output()
        .expect("the flowspace3 binary should run");
    server.join().expect("the stub server should exit");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, expected.as_bytes());
    assert!(output.stderr.is_empty());
}
