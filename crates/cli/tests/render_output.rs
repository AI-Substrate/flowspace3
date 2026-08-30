//! Human/JSON routing through the real binary.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

const TEST_KEY: &str = "isolated-render-test-key";

fn binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_flowspace3"))
}

fn goldens() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/goldens")
}

fn command(config: &Path) -> Command {
    write_test_key(config);
    fs3_testkit::sealed(&binary(), config, fs3_testkit::TestDatabase::Unreachable)
}

fn write_test_key(config: &Path) {
    let path = fs3_core::daemon_key_path(config);
    std::fs::write(&path, TEST_KEY).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
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

#[cfg(unix)]
#[derive(Clone, Copy)]
enum ScriptFlavor {
    Bsd,
    UtilLinux,
}

#[cfg(unix)]
fn script_flavor() -> ScriptFlavor {
    static FLAVOR: OnceLock<ScriptFlavor> = OnceLock::new();
    *FLAVOR.get_or_init(|| {
        let probe = |args: &[&str]| {
            Command::new("script")
                .args(args)
                .output()
                .is_ok_and(|output| {
                    output.status.success()
                        && String::from_utf8_lossy(&output.stdout).contains("fs3-script-probe")
                })
        };
        if probe(&["-q", "/dev/null", "printf", "fs3-script-probe"]) {
            ScriptFlavor::Bsd
        } else if probe(&["-qec", "printf fs3-script-probe", "/dev/null"]) {
            ScriptFlavor::UtilLinux
        } else {
            panic!(
                "script(1) is required to prove the TTY output matrix, but neither BSD nor util-linux invocation allocated a working terminal"
            );
        }
    })
}

#[cfg(unix)]
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(unix)]
fn run_tty(case: &Case, json: bool) -> Output {
    let config = tempfile::tempdir().unwrap();
    write_test_key(config.path());
    let mut cli_args = case
        .args
        .iter()
        .map(|arg| (*arg).to_string())
        .collect::<Vec<_>>();
    if json {
        cli_args.push("--json".to_string());
    }
    let server = case.response.map(|fixture| {
        let body = std::fs::read(goldens().join("responses").join(fixture)).unwrap();
        let (url, handle) = serve(body);
        cli_args.extend(["--daemon-url".to_string(), url]);
        handle
    });

    let mut script = fs3_testkit::sealed(
        Path::new("script"),
        config.path(),
        fs3_testkit::TestDatabase::Unreachable,
    );
    match script_flavor() {
        ScriptFlavor::Bsd => {
            script
                .args(["-q", "/dev/null"])
                .arg(binary())
                .args(&cli_args);
        }
        ScriptFlavor::UtilLinux => {
            let command = std::iter::once(binary().to_string_lossy().into_owned())
                .chain(cli_args)
                .map(|arg| shell_quote(&arg))
                .collect::<Vec<_>>()
                .join(" ");
            script.args(["-qec", &command, "/dev/null"]);
        }
    }
    let output = script.output().unwrap();
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
                    assert!(
                        request
                            .to_ascii_lowercase()
                            .contains(&format!("authorization: bearer {TEST_KEY}")),
                        "request did not carry the isolated daemon key: {request}"
                    );
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

#[cfg(unix)]
#[test]
fn every_covered_verb_obeys_both_real_tty_legs() {
    for case in CASES {
        let human = run_tty(case, false);
        let human_text = String::from_utf8_lossy(&human.stdout);
        assert!(
            human_text.contains(case.marker),
            "{} TTY default did not render marker {:?}:\n{human_text}",
            case.name,
            case.marker
        );
        assert!(
            !human_text.trim_start().starts_with('{'),
            "{} TTY default remained JSON:\n{human_text}",
            case.name
        );

        let json = run_tty(case, true);
        let json_text = String::from_utf8_lossy(&json.stdout).replace("\r\n", "\n");
        let json_start = json_text.find('{').expect("PTY transcript contains JSON");
        let json_end = json_text.rfind('}').expect("PTY transcript closes JSON") + 1;
        let json_document = &json_text[json_start..json_end];
        assert!(
            !json_text.contains('▍'),
            "{} --json rendered a human screen in a TTY:\n{json_text}",
            case.name
        );
        assert!(
            json_text.contains("\"command\""),
            "{} --json did not emit an envelope in a TTY:\n{json_text}",
            case.name
        );
        if case.exit == 0 {
            serde_json::from_str::<serde_json::Value>(json_document).unwrap_or_else(|error| {
                panic!(
                    "{} TTY --json was not JSON: {error}\n{json_text}",
                    case.name
                )
            });
        } else {
            assert!(json_text.contains("\"ok\": false"), "{json_text}");
        }
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
        .args(["search", "anything", "--source", "raw", "--human"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(!output.stderr.is_empty());
}

#[test]
fn agents_start_here_teaches_every_output_mode_rule() {
    let case = CASES
        .iter()
        .find(|case| case.name == "agents-start-here")
        .unwrap();
    let output = run(case, Mode::Default);
    assert_eq!(output.status.code(), Some(0));
    let envelope: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let text = envelope["data"]["text"].as_str().unwrap();
    for rule in [
        "TTY (terminal)",
        "pipe, file, CI capture",
        "JSON envelope with no flag",
        "`--json` forces",
        "`FS3_OUTPUT=json`",
    ] {
        assert!(text.contains(rule), "agent guide omitted {rule:?}:\n{text}");
    }
}
