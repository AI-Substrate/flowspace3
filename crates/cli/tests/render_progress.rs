use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::time::{Duration, Instant};

const TEST_KEY: &str = "isolated-render-test-key";

fn binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_flowspace3"))
}

fn serve() -> (String, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let envelope = include_bytes!("goldens/responses/add.json").to_vec();
    let events = include_bytes!("fixtures/scan-progress.ndjson").to_vec();
    let handle = std::thread::spawn(move || {
        let started = Instant::now();
        let mut event_seen = false;
        let mut post_seen = false;
        while started.elapsed() < Duration::from_secs(5) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    stream.set_nonblocking(false).ok();
                    let mut request = [0_u8; 4096];
                    let read = stream.read(&mut request).unwrap_or(0);
                    let request = String::from_utf8_lossy(&request[..read]);
                    assert!(
                        request
                            .to_ascii_lowercase()
                            .contains(&format!("authorization: bearer {TEST_KEY}")),
                        "request did not carry the isolated daemon key: {request}"
                    );
                    if request.starts_with("GET /events ") {
                        event_seen = true;
                        let events = events.clone();
                        std::thread::spawn(move || {
                            let head = b"HTTP/1.1 200 OK\r\nContent-Type: application/x-ndjson\r\nConnection: close\r\n\r\n";
                            let _ = stream.write_all(head);
                            let _ = stream.write_all(&events);
                            let _ = stream.flush();
                            std::thread::sleep(Duration::from_secs(2));
                        });
                    } else {
                        post_seen = true;
                        let envelope = envelope.clone();
                        std::thread::spawn(move || {
                            std::thread::sleep(Duration::from_millis(200));
                            let head = format!(
                                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                envelope.len()
                            );
                            let _ = stream.write_all(head.as_bytes());
                            let _ = stream.write_all(&envelope);
                        });
                    }
                    if event_seen && post_seen {
                        return;
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(2));
                }
                Err(_) => return,
            }
        }
    });
    (format!("http://{address}"), handle)
}

#[test]
fn add_progress_is_stderr_only_and_is_erased_without_waiting_for_the_stream() {
    let config = tempfile::tempdir().unwrap();
    let key_path = fs3_core::daemon_key_path(config.path());
    std::fs::write(&key_path, TEST_KEY).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(key_path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    let (url, server) = serve();
    let started = Instant::now();
    let output = fs3_testkit::sealed(
        &binary(),
        config.path(),
        fs3_testkit::TestDatabase::Unreachable,
    )
    .args(["add", ".", "--human", "--daemon-url", &url])
    .output()
    .unwrap();
    let elapsed = started.elapsed();
    server.join().unwrap();

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("456 files"), "{stdout}");
    assert!(
        !stdout.contains("1200 files"),
        "progress leaked to stdout: {stdout}"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("1200 files · 900 queued"), "{stderr:?}");
    assert!(
        stderr.ends_with("\r\u{1b}[2K"),
        "meter was not erased: {stderr:?}"
    );
    assert!(
        elapsed < Duration::from_secs(1),
        "POST waited for the 2s event stream: {elapsed:?}"
    );
}
