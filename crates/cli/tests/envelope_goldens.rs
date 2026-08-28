//! The agent contract, asserted as BYTES.
//!
//! # What this proves
//!
//! Plan 007 adds a human-readable presentation layer to a CLI that has, until
//! now, only ever printed JSON. The whole plan rests on one promise: **an agent
//! sees exactly what it saw before**. A promise like that is worth nothing as
//! prose, so this file turns it into a mechanical predicate — for a fixed
//! daemon answer, the bytes on stdout are the bytes in `goldens/stdout/`.
//!
//! # Why a stub daemon rather than a seeded store
//!
//! The goldens must be reproducible on any machine, in CI, offline. A real
//! store cannot give that: `worktree_id` is a serial
//! (`crates/daemon/src/roots.rs:48-60`), queue counts move while the runner
//! drains, `last_error` is `ORDER BY updated_at DESC`
//! (`crates/store/src/jobs.rs:479-486`), and root paths are whatever the host
//! has registered. So determinism is bought where it is cheapest and most
//! honest: the daemon's ANSWER is frozen as a file, and the thing under test is
//! the CLI's output path — client parse, envelope, `emit`. That is precisely
//! the surface plan 007 touches; nothing else about a golden would be evidence.
//!
//! The frozen answers in `goldens/responses/` are real captures from a live
//! daemon (status, search, get, tree, conversation list, and two failures)
//! plus faithful synthetic bodies for the verbs that mutate — see
//! `goldens/PROVENANCE.md`, which names the origin of every one.
//!
//! # How the goldens were captured, and why that matters
//!
//! By this harness, driving the PRE-PLAN binary — `main` at `1ce572b`, before
//! a line of plan-007 code existed:
//!
//! ```console
//! FS3_GOLDEN_BIN=/path/to/pre-plan/target/debug/flowspace3 \
//!   FS3_GOLDEN_UPDATE=1 cargo test -p fs3-cli --test envelope_goldens
//! ```
//!
//! A witness minted by the code it is meant to police is not a witness. One
//! harness, two binaries: the goldens carry the old binary's bytes, and every
//! run from here on asserts the current one against them.
//!
//! Re-capturing is therefore never a fix for a red test here. A diff means the
//! agent-facing envelope moved, which is a product decision (o-prime's), not a
//! test-maintenance chore.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

/// Where the frozen daemon answers and the expected stdout live.
fn goldens_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/goldens")
}

/// The binary under test.
///
/// `FS3_GOLDEN_BIN` exists for exactly one job: capturing the goldens from a
/// build of a DIFFERENT commit through this same harness. It is read by the
/// test process, never handed to the child — `fs3_testkit::sealed` scrubs
/// `FS3_*` out of the child's environment regardless.
fn binary_under_test() -> PathBuf {
    match std::env::var("FS3_GOLDEN_BIN") {
        Ok(path) if !path.is_empty() => PathBuf::from(path),
        _ => PathBuf::from(env!("CARGO_BIN_EXE_flowspace3")),
    }
}

/// A `flowspace3` that cannot reach the production store.
///
/// Sealed for the reason `fs3_testkit::spawn` documents: unsealed, these spawns
/// read the developer's real `config.toml` and real `secrets.env`, and an
/// ambient `FS3_*` would silently change the bytes this file is asserting on.
fn flowspace3(config_dir: &Path) -> Command {
    fs3_testkit::sealed(
        &binary_under_test(),
        config_dir,
        fs3_testkit::TestDatabase::Unreachable,
    )
}

/// Answer one request with `body`, then stop — and stop anyway if no request
/// ever comes.
///
/// Hand-rolled, in the shape `crates/cli/tests/ping.rs` established: the
/// contract being exercised is "the daemon returns these bytes", and a socket
/// says that with nothing in between.
///
/// The deadline is not decoration. A case whose arguments clap REJECTS never
/// opens a connection, and a blocking `accept()` then hangs the suite forever
/// with no clue which case did it — that cost this harness one 900-second
/// timeout before the loop below was bounded.
///
/// The status is always 200: `DaemonClient::envelope`
/// (`crates/cli/src/client.rs:219-261`) parses the body whatever the status
/// says, so the code adds nothing a golden could observe.
fn serve(body: Vec<u8>) -> (String, std::thread::JoinHandle<()>) {
    /// Longer than any local request needs, far shorter than the CLI's own
    /// 300-second scan timeout (`crates/cli/src/client.rs:67`).
    const DEADLINE: Duration = Duration::from_secs(20);

    let listener = TcpListener::bind("127.0.0.1:0").expect("an ephemeral port");
    let address = listener.local_addr().expect("the socket is bound");
    listener
        .set_nonblocking(true)
        .expect("the listener accepts a nonblocking mode");

    let handle = std::thread::spawn(move || {
        let started = Instant::now();
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
                    if started.elapsed() > DEADLINE {
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(_) => return,
            }
        }
    });

    (format!("http://{address}"), handle)
}

/// One golden: a name, the verb's arguments, the daemon answer it is fed, and
/// the exit code it must produce.
///
/// `response` is `None` for the verbs the CLI answers by itself — those talk to
/// no daemon at all, and their bytes are just as much the agent contract.
///
/// `exit` is here because an agent branches on it before it parses anything:
/// 0 ok, 1 a rendered failure, 2 a usage problem (`crates/cli/src/main.rs:318`).
/// A renderer that printed beautifully and exited 0 on a failure would pass a
/// stdout-only assertion and still have broken every script in the wild.
struct Case {
    name: &'static str,
    args: &'static [&'static str],
    response: Option<&'static str>,
    exit: i32,
}

/// Every covered verb, in the order `plan.dd.md` covers them.
///
/// `doctor` is deliberately absent: its payload carries `elapsed_ms` from a
/// live `Instant` (`crates/cli/src/doctor.rs:60-94`), so it has no byte-stable
/// form to freeze. `ping` is absent because it prints prose, not an envelope
/// (`crates/cli/src/main.rs:627-647`); `config show` likewise. See
/// `goldens/PROVENANCE.md` § Not covered.
const CASES: &[Case] = &[
    Case {
        name: "status",
        args: &["status"],
        response: Some("status.json"),
        exit: 0,
    },
    Case {
        name: "status-with-messages",
        args: &["status"],
        response: Some("messages.json"),
        exit: 0,
    },
    Case {
        name: "search",
        args: &["search", "render the envelope for a human", "--limit", "3"],
        response: Some("search.json"),
        exit: 0,
    },
    Case {
        name: "get",
        args: &[
            "get",
            "el:git:github.com/AI-Substrate/flowspace3/crates/cli/src/main.rs::emit",
        ],
        response: Some("get.json"),
        exit: 0,
    },
    Case {
        name: "tree",
        args: &["tree", "--limit", "3"],
        response: Some("tree.json"),
        exit: 0,
    },
    Case {
        name: "add",
        args: &["add", "."],
        response: Some("add.json"),
        exit: 0,
    },
    Case {
        name: "scan",
        args: &["scan", "."],
        response: Some("scan.json"),
        exit: 0,
    },
    Case {
        name: "remove",
        args: &["remove", "."],
        response: Some("remove.json"),
        exit: 0,
    },
    Case {
        name: "gc",
        args: &["gc"],
        response: Some("gc.json"),
        exit: 0,
    },
    Case {
        name: "conversation-list",
        args: &["conversation", "list"],
        response: Some("conversation-list.json"),
        exit: 0,
    },
    Case {
        name: "error-not-found",
        args: &["get", "el:git:nope/nope::nope"],
        response: Some("error-not-found.json"),
        exit: 1,
    },
    Case {
        name: "error-query-empty",
        args: &["search", ""],
        response: Some("error-query-empty.json"),
        exit: 1,
    },
    Case {
        name: "docs-list",
        args: &["docs", "list"],
        response: None,
        exit: 0,
    },
    Case {
        name: "agents-start-here",
        args: &["agents-start-here"],
        response: None,
        exit: 0,
    },
    // A usage problem is answered by clap, before any envelope exists: exit 2,
    // stderr, and — the line that matters for plan 007 — NOTHING on stdout. An
    // agent that pipes stdout must keep getting an empty stream here, not a
    // helpful human paragraph.
    Case {
        name: "usage-error-prints-no-envelope",
        args: &["search", "anything", "--source", "code"],
        response: None,
        exit: 2,
    },
];

/// Run one case; return what it printed to stdout and how it exited.
fn run(case: &Case) -> (Vec<u8>, Option<i32>) {
    let config = tempfile::tempdir().expect("a temp config directory");
    let mut command = flowspace3(config.path());
    command.args(case.args);

    let server = case.response.map(|fixture| {
        let body =
            std::fs::read(goldens_dir().join("responses").join(fixture)).unwrap_or_else(|error| {
                panic!("the frozen answer {fixture} should be readable: {error}")
            });
        let (url, handle) = serve(body);
        command.args(["--daemon-url", &url]);
        handle
    });

    let output = command
        .output()
        .expect("the flowspace3 binary under test should run");

    if let Some(server) = server {
        server.join().ok();
    }

    (output.stdout, output.status.code())
}

/// The one test: every covered verb still prints the bytes it printed on
/// pre-plan main.
///
/// One test rather than fourteen so that a drift report names EVERY verb that
/// moved. A rendering change that touches one verb and a serialisation change
/// that touches all of them are different diagnoses, and a per-case test would
/// report the first and hide the second behind it.
#[test]
fn the_piped_envelope_is_byte_identical_to_pre_plan_main() {
    let updating = std::env::var("FS3_GOLDEN_UPDATE").is_ok_and(|value| !value.is_empty());
    let stdout_dir = goldens_dir().join("stdout");
    let mut drifted = Vec::new();

    for case in CASES {
        let (actual, exit) = run(case);
        let golden = stdout_dir.join(format!("{}.stdout", case.name));

        if updating {
            std::fs::write(&golden, &actual).expect("the golden should be writable");
            continue;
        }

        assert_eq!(
            exit,
            Some(case.exit),
            "{} exited {exit:?}, not {}: the exit code is the first thing a script reads",
            case.name,
            case.exit
        );

        let expected = std::fs::read(&golden).unwrap_or_else(|error| {
            panic!(
                "{} has no golden at {}: {error} — capture it from pre-plan main, never from this \
                 build",
                case.name,
                golden.display()
            )
        });

        if actual != expected {
            drifted.push(format!(
                "--- {} ---\nexpected ({} bytes):\n{}\nactual ({} bytes):\n{}",
                case.name,
                expected.len(),
                String::from_utf8_lossy(&expected),
                actual.len(),
                String::from_utf8_lossy(&actual),
            ));
        }
    }

    assert!(
        !updating,
        "FS3_GOLDEN_UPDATE was set: {} golden(s) were rewritten. This is a capture run, and it \
         must be done with FS3_GOLDEN_BIN pointed at a pre-plan build — never let a capture run \
         count as a passing gate.",
        CASES.len()
    );

    assert!(
        drifted.is_empty(),
        "the agent-facing envelope MOVED for {} verb(s). These bytes are the product's contract \
         with every agent and every `| jq` in the wild; a diff here is a product decision, not a \
         golden to re-capture.\n\n{}",
        drifted.len(),
        drifted.join("\n\n")
    );
}
