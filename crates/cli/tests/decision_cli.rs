//! `flowspace3 decision`, end to end through the real binary, against a stub
//! Decisions endpoint configured as an `openai_compat` provider.
//!
//! No key, no network: the stub is a hand-rolled one-shot HTTP server, the
//! key is a throwaway value in a variable only this test sets.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::Command;
use std::sync::mpsc;

const KEY_ENV: &str = "DECISION_CLI_TEST_KEY";

fn flowspace3(config_dir: &Path) -> Command {
    fs3_testkit::sealed(
        Path::new(env!("CARGO_BIN_EXE_flowspace3")),
        config_dir,
        fs3_testkit::TestDatabase::Unreachable,
    )
}

/// One request in, `(status, body)` out; the request line, headers and body
/// come back on the channel.
fn serve_once(status: &'static str, body: &'static str) -> (String, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("an ephemeral port");
    let address = listener.local_addr().expect("the socket is bound");
    let (sender, receiver) = mpsc::channel();

    std::thread::spawn(move || {
        let Ok((stream, _)) = listener.accept() else {
            return;
        };
        let mut reader = BufReader::new(stream);
        let mut head = String::new();
        let mut length = 0;
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                break;
            }
            if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                length = value.trim().parse().unwrap_or(0);
            }
            head.push_str(&line);
        }
        let mut payload = vec![0; length];
        let _ = reader.read_exact(&mut payload);
        let _ = sender.send(format!("{head}\n{}", String::from_utf8_lossy(&payload)));

        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let mut stream = reader.into_inner();
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.flush();
    });

    (format!("http://{address}"), receiver)
}

fn configure(config_dir: &Path, base_url: &str) {
    std::fs::write(
        config_dir.join("config.toml"),
        format!(
            "[providers.gw]\nkind = \"openai_compat\"\nbase_url = \"{base_url}\"\n\
             model = \"gpt-6-luna\"\napi_key_env = \"{KEY_ENV}\"\n"
        ),
    )
    .expect("writing the test config");
}

fn envelope(output: &std::process::Output) -> serde_json::Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "stdout is not an envelope ({error}): {}",
            String::from_utf8_lossy(&output.stdout)
        )
    })
}

#[test]
fn a_choice_with_an_attachment_round_trips_through_the_provider() {
    let (url, request) = serve_once(
        "200 OK",
        r#"{"model":"gpt-6-luna","answers":[{"type":"choice","name":"team","choice":"billing","confidence":0.93,"probabilities":[{"value":"billing","probability":0.95},{"value":"other","probability":0.05}]}],"usage":{"input_tokens":88,"output_tokens":0,"total_tokens":88}}"#,
    );
    let config = tempfile::tempdir().expect("a temp config directory");
    configure(config.path(), &url);
    let image = config.path().join("receipt.png");
    std::fs::write(&image, b"\x89PNG").expect("writing a stand-in image");

    let output = flowspace3(config.path())
        .env(KEY_ENV, "stub-key")
        .args([
            "decision",
            "Which team handles this?",
            "--name",
            "team",
            "--choice",
            "billing=charges and refunds",
            "--choice",
            "other",
            "--input",
            "I was charged twice.",
            "--attach",
        ])
        .arg(&image)
        .args(["--provider", "gw", "--json"])
        .output()
        .expect("the binary runs");

    let envelope = envelope(&output);
    assert!(output.status.success(), "{envelope}");
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["command"], "decision");
    assert_eq!(envelope["data"]["answers"][0]["choice"], "billing");
    assert_eq!(envelope["data"]["questions"][0]["type"], "choice");
    assert_eq!(envelope["data"]["input"]["images"], 1);
    assert_eq!(envelope["data"]["cost"]["source"], "estimate");
    assert!(
        envelope["data"]["cost"]["total_usd"]
            .as_f64()
            .is_some_and(|usd| (usd - 0.000_008_8).abs() < 1e-12),
        "88 input tokens at $0.10/1M: {envelope}"
    );
    assert!(envelope["next_action"].is_string());

    let seen = request.recv().expect("the stub saw a request");
    assert!(seen.starts_with("POST /decisions "), "{seen}");
    assert!(
        seen.to_ascii_lowercase()
            .contains("authorization: bearer stub-key"),
        "{seen}"
    );
    let body: serde_json::Value =
        serde_json::from_str(seen.split_once("\r\n\n").map_or("", |(_, body)| body))
            .expect("a JSON body");
    assert_eq!(body["model"], "gpt-6-luna");
    assert_eq!(
        body["questions"][0]["choices"][0]["description"],
        "charges and refunds"
    );
    assert_eq!(
        body["input"][0]["content"][0]["text"],
        "I was charged twice."
    );
    assert!(
        body["input"][0]["content"][1]["image_url"]
            .as_str()
            .is_some_and(|url| url.starts_with("data:image/png;base64,")),
        "{body}"
    );
}

#[test]
fn a_rejection_is_an_error_envelope_with_the_field_to_fix() {
    let (url, _request) = serve_once(
        "400 Bad Request",
        r#"{"error":{"message":"Invalid 'questions[0].choices': array too short.","type":"invalid_request_error","param":"questions[0].choices","code":"array_below_min_length"}}"#,
    );
    let config = tempfile::tempdir().expect("a temp config directory");
    configure(config.path(), &url);

    let output = flowspace3(config.path())
        .env(KEY_ENV, "stub-key")
        .args([
            "decision",
            "Pick",
            "--choice",
            "only",
            "--provider",
            "gw",
            "--json",
        ])
        .output()
        .expect("the binary runs");

    let envelope = envelope(&output);
    assert_eq!(output.status.code(), Some(1), "{envelope}");
    assert_eq!(envelope["ok"], false);
    assert_eq!(envelope["error"]["code"], "FS3-E-PROVIDER-FAILED");
    assert_eq!(
        envelope["error"]["details"]["param"],
        "questions[0].choices"
    );
    assert_eq!(envelope["error"]["retryable"], false);
    assert!(
        envelope["error"]["fix"]
            .as_str()
            .is_some_and(|fix| fix.contains("questions[0].choices")),
        "{envelope}"
    );
}

#[test]
fn a_missing_key_fails_before_any_request_and_names_the_variable() {
    let config = tempfile::tempdir().expect("a temp config directory");

    let output = flowspace3(config.path())
        .env_remove("OPENAI_API_KEY")
        .env_remove("OPEN_AI")
        .args(["decision", "Is this fine?", "--json"])
        .output()
        .expect("the binary runs");

    let envelope = envelope(&output);
    assert_eq!(output.status.code(), Some(1), "{envelope}");
    assert_eq!(envelope["error"]["code"], "FS3-E-PROVIDER-FAILED");
    assert!(
        envelope["error"]["fix"]
            .as_str()
            .is_some_and(|fix| fix.contains("OPENAI_API_KEY")),
        "{envelope}"
    );
}

#[test]
fn choice_and_level_together_are_a_usage_error() {
    let config = tempfile::tempdir().expect("a temp config directory");

    let output = flowspace3(config.path())
        .args(["decision", "?", "--choice", "a", "--level", "b"])
        .output()
        .expect("the binary runs");

    assert_eq!(output.status.code(), Some(2));
}
