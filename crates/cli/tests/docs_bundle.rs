//! The bundled docs must not teach commands that do not exist.
//!
//! Self-teaching documentation that teaches the wrong thing is worse than none,
//! because the reader trusts it — and the reader here is an agent that will run
//! what it is told, get a usage error, and have no way to tell "I misread the
//! docs" from "the docs are wrong". A renamed verb has to break this test, not
//! a user's session.
//!
//! Same muscle as the architecture check and the error-code drift test: the
//! thing that could rot is checked mechanically rather than remembered.

use std::collections::BTreeSet;
use std::process::Command;

use fs3_cli::docs;

/// The binary under test, built by cargo for this integration test.
const FLOWSPACE3: &str = env!("CARGO_BIN_EXE_flowspace3");

/// Every `flowspace3 <verb>` mentioned anywhere in the bundle.
///
/// Deliberately crude — a scan for the literal prefix, taking the next word —
/// because the failure being caught is a renamed or removed verb, and anything
/// cleverer would be a markdown parser with its own bugs.
fn commands_mentioned() -> BTreeSet<String> {
    const PREFIX: &str = "flowspace3 ";
    let mut found = BTreeSet::new();

    for topic in docs::TOPICS {
        let mut rest = topic.text;
        while let Some(at) = rest.find(PREFIX) {
            rest = &rest[at + PREFIX.len()..];
            let verb: String = rest
                .chars()
                .take_while(|c| c.is_ascii_lowercase() || *c == '-')
                .collect();
            if !verb.is_empty() {
                found.insert(verb);
            }
        }
    }
    found
}

/// The subcommands clap actually offers, read from `--help`.
///
/// Asked of the REAL binary rather than of a list in this test: a list here
/// would be a third place to keep in sync, and it would happily agree with docs
/// that are both wrong.
fn commands_offered() -> BTreeSet<String> {
    let help = Command::new(FLOWSPACE3)
        .arg("--help")
        .output()
        .expect("the binary should run");
    let text = String::from_utf8_lossy(&help.stdout);

    // clap lists subcommands one per line, indented, name first.
    text.lines()
        .skip_while(|line| !line.starts_with("Commands:"))
        .skip(1)
        .take_while(|line| line.starts_with("  "))
        .filter_map(|line| line.split_whitespace().next())
        .filter(|word| word.chars().all(|c| c.is_ascii_lowercase() || c == '-'))
        .map(str::to_string)
        .collect()
}

#[test]
fn every_command_the_bundle_teaches_actually_exists() {
    let offered = commands_offered();
    assert!(
        !offered.is_empty(),
        "could not read subcommands from --help; the parser above needs updating"
    );

    for verb in commands_mentioned() {
        assert!(
            offered.contains(&verb),
            "the bundled docs tell a reader to run `flowspace3 {verb}`, which is not a \
             subcommand. Offered: {offered:?}.\nEither the verb was renamed and the pages \
             need updating, or the page has a typo — an agent cannot tell the difference, \
             which is why this fails here."
        );
    }
}

/// The other direction is a warning, not a failure: a verb nobody documents is
/// a gap worth seeing, but `ping` and `help` are legitimately undocumented
/// plumbing, so this asserts only that the LOAD-BEARING loop is covered.
#[test]
fn the_operating_loop_is_documented() {
    let mentioned = commands_mentioned();
    for verb in [
        "doctor", "daemon", "add", "status", "search", "get", "tree", "docs",
    ] {
        assert!(
            mentioned.contains(verb),
            "`{verb}` is part of the operating loop but no bundled page mentions it"
        );
    }
}

/// `docs get` has to work from the shipped artifact with no daemon, no network
/// and no database — that is the whole promise of bundling.
#[test]
fn docs_answer_from_the_binary_with_nothing_else_running() {
    let output = Command::new(FLOWSPACE3)
        .args(["docs", "get", "agents"])
        // A config directory that does not exist, and a database URL nothing
        // serves: if either mattered, this would fail.
        .env("FS3_CONFIG_DIR", "/nonexistent/fs3-docs-test")
        .env("FS3_DATABASE__URL", "postgres://nobody@127.0.0.1:1/nothing")
        .output()
        .expect("the binary should run");

    assert!(
        output.status.success(),
        "docs must answer offline; stderr was: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let envelope: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("docs answers an envelope");
    assert_eq!(envelope["ok"], serde_json::json!(true));
    assert_eq!(envelope["command"], serde_json::json!("docs"));
    assert_eq!(envelope["data"]["topic"], serde_json::json!("agents"));
    assert!(
        envelope["data"]["text"]
            .as_str()
            .expect("text is a string")
            .contains("ok` is the ONLY discriminator"),
        "the agents page must carry the envelope contract — it is the thing a new \
         consumer most needs and most easily gets wrong"
    );
}

#[test]
fn an_unknown_topic_exits_non_zero_and_names_the_real_ones() {
    let output = Command::new(FLOWSPACE3)
        .args(["docs", "get", "embeddings"])
        .output()
        .expect("the binary should run");

    assert!(!output.status.success(), "a bad topic is a failure exit");

    let envelope: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("even failures are envelopes on stdout");
    assert_eq!(envelope["ok"], serde_json::json!(false));
    assert_eq!(
        envelope["error"]["code"],
        serde_json::json!("FS3-E-USAGE-TOPIC-NOT-FOUND")
    );
    let fix = envelope["error"]["fix"].as_str().expect("fix is mandatory");
    assert!(fix.contains("agents"), "the fix lists the topics: {fix}");
}
