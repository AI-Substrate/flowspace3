//! Human/JSON routing through the real binary.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, Instant};

fn binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_flowspace3"))
}

fn goldens() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/goldens")
}

fn command(config: &Path) -> Command {
    fs3_testkit::sealed(&binary(), config, fs3_testkit::TestDatabase::Unreachable)
}

struct Case {
    name: &'static str,
    args: &'static [&'static str],
    response: Option<&'static str>,
    marker: &'static str,
    exit: i32,
}

const CASES: &[Case] = &[
    Case {
        name: "status",
        args: &["status"],
        response: Some("status.json"),
        marker: "roots",
        exit: 0,
    },
    Case {
        name: "search",
        args: &["search", "render the envelope for a human", "--limit", "3"],
        response: Some("search.json"),
        marker: "score",
        exit: 0,
    },
    Case {
        name: "get",
        args: &[
            "get",
            "el:git:github.com/AI-Substrate/flowspace3/crates/cli/src/main.rs::emit",
        ],
        response: Some("get.json"),
        marker: "fn emit",
        exit: 0,
    },
    Case {
        name: "tree",
        args: &["tree", "--limit", "3"],
        response: Some("tree.json"),
        marker: "repository",
        exit: 0,
    },
    Case {
        name: "add",
        args: &["add", "."],
        response: Some("add.json"),
        marker: "files",
        exit: 0,
    },
    Case {
        name: "scan",
        args: &["scan", "."],
        response: Some("scan.json"),
        marker: "unchanged",
        exit: 0,
    },
    Case {
        name: "remove",
        args: &["remove", "."],
        response: Some("remove.json"),
        marker: "jobs killed",
        exit: 0,
    },
    Case {
        name: "gc",
        args: &["gc"],
        response: Some("gc.json"),
        marker: "rows reclaimed",
        exit: 0,
    },
    Case {
        name: "conversation-list",
        args: &["conversation", "list"],
        response: Some("conversation-list.json"),
        marker: "no indexed conversations",
        exit: 0,
    },
    Case {
        name: "error-not-found",
        args: &["get", "el:git:nope/nope::nope"],
        response: Some("error-not-found.json"),
        marker: "fix",
        exit: 1,
    },
    Case {
        name: "docs-list",
        args: &["docs", "list"],
        response: None,
        marker: "topics",
        exit: 0,
    },
    Case {
        name: "agents-start-here",
        args: &["agents-start-here"],
        response: None,
        marker: "agent",
        exit: 0,
    },
];

#[derive(Clone, Copy)]
enum Mode {
    Default,
    HumanFlag,
    JsonUnderHumanEnv,
    HumanEnv,
}

fn run(case: &Case, mode: Mode) -> Output {
    let config = tempfile::tempdir().unwrap();
    let mut child = command(config.path());
    child.args(case.args);
    match mode {
        Mode::Default => {}
        Mode::HumanFlag => {
            child.arg("--human");
        }
        Mode::JsonUnderHumanEnv => {
            child.arg("--json").env("FS3_OUTPUT", "human");
        }
        Mode::HumanEnv => {
            child.env("FS3_OUTPUT", "human");
        }
    }

    let server = case.response.map(|fixture| {
        let body = std::fs::read(goldens().join("responses").join(fixture)).unwrap();
        let (url, handle) = serve(body);
        child.args(["--daemon-url", &url]);
        handle
    });
    let output = child.output().unwrap();
    if let Some(server) = server {
        server.join().unwrap();
    }
    output
}

fn serve(body: Vec<u8>) -> (String, std::thread::JoinHandle<()>) {
    const DEADLINE: Duration = Duration::from_secs(20);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let handle = std::thread::spawn(move || {
        let started = Instant::now();
        while started.elapsed() < DEADLINE {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    stream.set_nonblocking(false).ok();
                    let mut request = [0_u8; 4096];
                    let read = stream.read(&mut request).unwrap_or(0);
                    let request = String::from_utf8_lossy(&request[..read]);
                    if request.starts_with("GET /events ") {
                        let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                        continue;
                    }
                    let head = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = stream.write_all(head.as_bytes());
                    let _ = stream.write_all(&body);
                    return;
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(2))
                }
                Err(_) => return,
            }
        }
    });
    (format!("http://{address}"), handle)
}

#[test]
fn every_covered_daemon_verb_obeys_the_four_case_matrix() {
    for case in CASES {
        let default = run(case, Mode::Default);
        assert_eq!(
            default.status.code(),
            Some(case.exit),
            "{} default exit",
            case.name
        );
        let editorial = matches!(case.name, "docs-list" | "agents-start-here");
        let expected = if editorial {
            assert!(
                serde_json::from_slice::<serde_json::Value>(&default.stdout).is_ok(),
                "{} piped default is not JSON",
                case.name
            );
            default.stdout.clone()
        } else {
            let golden = std::fs::read(
                goldens()
                    .join("stdout")
                    .join(format!("{}.stdout", case.name)),
            )
            .unwrap();
            assert_eq!(default.stdout, golden, "{} piped default moved", case.name);
            golden
        };

        let human = run(case, Mode::HumanFlag);
        assert_eq!(
            human.status.code(),
            Some(case.exit),
            "{} --human exit",
            case.name
        );
        let text = String::from_utf8_lossy(&human.stdout);
        assert!(
            text.contains(case.marker),
            "{} human marker {:?} absent:\n{text}",
            case.name,
            case.marker
        );
        assert!(
            serde_json::from_slice::<serde_json::Value>(&human.stdout).is_err(),
            "{} --human remained JSON",
            case.name
        );

        let forced_json = run(case, Mode::JsonUnderHumanEnv);
        assert_eq!(
            forced_json.stdout, expected,
            "{} --json did not win",
            case.name
        );

        let env_human = run(case, Mode::HumanEnv);
        let text = String::from_utf8_lossy(&env_human.stdout);
        assert!(
            text.contains(case.marker),
            "{} FS3_OUTPUT=human marker absent:\n{text}",
            case.name
        );
    }
}

#[test]
fn doctor_obeys_the_matrix_without_touching_a_real_container_engine() {
    for mode in [
        Mode::Default,
        Mode::HumanFlag,
        Mode::JsonUnderHumanEnv,
        Mode::HumanEnv,
    ] {
        let config = tempfile::tempdir().unwrap();
        let mut child = command(config.path());
        child
            .args(["doctor", "--config-dir"])
            .arg(config.path())
            .env("FS3_ENGINE", "/usr/bin/false");
        match mode {
            Mode::Default => {}
            Mode::HumanFlag => {
                child.arg("--human");
            }
            Mode::JsonUnderHumanEnv => {
                child.arg("--json").env("FS3_OUTPUT", "human");
            }
            Mode::HumanEnv => {
                child.env("FS3_OUTPUT", "human");
            }
        }
        let output = child.output().unwrap();
        assert_eq!(output.status.code(), Some(1));
        let json = serde_json::from_slice::<serde_json::Value>(&output.stdout).is_ok();
        assert_eq!(
            json,
            matches!(mode, Mode::Default | Mode::JsonUnderHumanEnv)
        );
        if !json {
            assert!(String::from_utf8_lossy(&output.stdout).contains("fix"));
        }
    }
}

#[test]
fn a_human_mode_usage_error_still_writes_nothing_to_stdout() {
    let config = tempfile::tempdir().unwrap();
    let output = command(config.path())
        .args(["search", "anything", "--source", "code", "--human"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(!output.stderr.is_empty());
}
