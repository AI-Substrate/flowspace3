//! End-to-end: the four surfaces, and the TTY strategy, through the real
//! binary.
//!
//! The unit tests beside each surface prove what a screen SAYS. These prove the
//! two properties that only exist once everything is wired together:
//!
//! 1. every fixture renders through the public entry point, and
//! 2. the JSON path is the same bytes that went in — not a re-serialisation.
//!
//! Property 2 is the prototype's whole thesis in an assertion. The moment the
//! human path and the machine path stop agreeing, the human skin has started
//! carrying truth of its own.

use std::process::Command;

use human_render::{Envelope, RenderOptions, render};
use serde_json::Value;

/// The fixtures, by name, exactly as the demo embeds them.
const FIXTURES: [(&str, &str); 4] = [
    ("search", include_str!("../fixtures/search.json")),
    ("doctor", include_str!("../fixtures/doctor.json")),
    ("error", include_str!("../fixtures/error.json")),
    ("status", include_str!("../fixtures/status.json")),
];

/// Strip the styling, the way a test (or a pipe) sees the output.
fn plain(styled: &str) -> String {
    anstream::adapter::strip_str(styled).to_string()
}

/// Run the demo binary. Its stdout is a pipe here, which IS the test: the
/// binary must make its own decision about that without being told.
fn demo(args: &[&str]) -> (String, bool) {
    let output = Command::new(env!("CARGO_BIN_EXE_demo"))
        .args(args)
        .output()
        .expect("the demo binary builds with the test");
    (
        String::from_utf8(output.stdout).expect("demo output is utf-8"),
        output.status.success(),
    )
}

#[test]
fn every_fixture_renders_something_a_person_can_read() {
    for (name, bytes) in FIXTURES {
        let envelope: Envelope<Value> =
            serde_json::from_str(bytes).unwrap_or_else(|error| panic!("{name}: {error}"));
        let screen = plain(&render(&envelope, &RenderOptions::default()));
        assert!(
            screen.lines().count() > 3,
            "{name} rendered almost nothing:\n{screen}"
        );
        assert!(
            screen.contains(&envelope.command),
            "{name} never names its verb:\n{screen}"
        );
    }
}

#[test]
fn a_pipe_gets_the_envelope_and_a_terminal_would_get_the_table() {
    // No flags, stdout is a pipe: the default must be the machine shape.
    let (piped, ok) = demo(&["--surface", "status"]);
    assert!(ok);
    let parsed: Value = serde_json::from_str(piped.trim()).expect("a pipe must receive JSON");
    assert_eq!(parsed["command"], "status");

    // `--human` overrides the detection, which is how a person captures a
    // transcript into a file.
    let (rich, ok) = demo(&["--surface", "status", "--human", "--width", "100"]);
    assert!(ok);
    assert!(rich.contains('╭'), "expected a table:\n{rich}");
}

#[test]
fn the_json_path_is_the_same_bytes_not_a_reserialisation() {
    for (name, bytes) in FIXTURES {
        let (piped, _) = demo(&["--surface", name, "--json"]);
        assert_eq!(
            piped.trim_end(),
            bytes.trim_end(),
            "{name}: the machine path must not rewrite the envelope"
        );
    }
}

#[test]
fn a_failure_envelope_exits_non_zero_even_when_it_renders_beautifully() {
    let (screen, ok) = demo(&["--surface", "error", "--human", "--width", "100"]);
    assert!(!ok, "a rendered failure is still a failure to the shell");
    assert!(screen.contains("fix"), "{screen}");
}

#[test]
fn no_color_strips_the_styling_without_changing_the_layout() {
    // NO_COLOR is anstream's business, not the renderer's; prove the renderer
    // did not quietly grow an opinion about it.
    let output = Command::new(env!("CARGO_BIN_EXE_demo"))
        .args(["--surface", "doctor", "--human", "--width", "100"])
        .env("NO_COLOR", "1")
        .output()
        .expect("the demo binary runs");
    let screen = String::from_utf8(output.stdout).expect("utf-8");
    assert!(!screen.contains('\u{1b}'), "NO_COLOR left sequences behind");

    let (colored, _) = demo(&[
        "--surface",
        "doctor",
        "--human",
        "--width",
        "100",
        "--color",
        "always",
    ]);
    assert!(colored.contains('\u{1b}'), "--color always emitted none");
    assert_eq!(
        plain(&colored),
        screen,
        "colour must not change what is on screen, only how it looks"
    );
}

#[test]
fn an_envelope_arriving_on_stdin_renders_like_any_other() {
    // The shape a promoted `fs3-cli` would actually use: bytes off a socket,
    // rendered by the same code path the fixtures take.
    use std::io::Write;
    use std::process::Stdio;

    let mut child = Command::new(env!("CARGO_BIN_EXE_demo"))
        .args(["--file", "-", "--human", "--width", "100"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn demo");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(br#"{"ok":true,"command":"add","v":1,"data":{"root":"/srv/api","files":91}}"#)
        .expect("write stdin");
    let output = child.wait_with_output().expect("demo finishes");
    let screen = plain(&String::from_utf8(output.stdout).expect("utf-8"));
    assert!(screen.contains("add"), "{screen}");
    assert!(screen.contains("/srv/api"), "{screen}");
}

#[test]
fn a_narrow_terminal_still_produces_a_table_that_fits() {
    let (screen, _) = demo(&["--surface", "search", "--human", "--width", "60"]);
    let screen = plain(&screen);
    let widest = screen
        .lines()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0);
    assert!(widest <= 60, "a 60-column canvas produced {widest} columns");
}
