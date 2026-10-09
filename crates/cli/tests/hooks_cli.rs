//! `flowspace3 hooks`: the installer and the prompt hook, run as a user and a
//! harness run them — the real binary, with `HOME`, `CODEX_HOME` and
//! `COPILOT_HOME` all inside a temp dir, so no real harness config is read or
//! written, and a daemon URL nothing listens on.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use serde_json::Value;

struct Box_ {
    dir: tempfile::TempDir,
}

impl Box_ {
    fn new(harness_roots: &[&str]) -> Self {
        let dir = tempfile::tempdir().unwrap();
        for root in harness_roots {
            std::fs::create_dir_all(dir.path().join("home").join(root)).unwrap();
        }
        std::fs::create_dir_all(dir.path().join("repo/.git")).unwrap();
        Self { dir }
    }

    fn home(&self) -> PathBuf {
        self.dir.path().join("home")
    }

    fn flowspace3(&self, args: &[&str]) -> Command {
        let mut command = fs3_testkit::sealed(
            Path::new(env!("CARGO_BIN_EXE_flowspace3")),
            self.dir.path(),
            fs3_testkit::TestDatabase::Unreachable,
        );
        command
            .args(args)
            .env("CODEX_HOME", self.home().join(".codex"))
            .env("COPILOT_HOME", self.home().join(".copilot"))
            .env("FS3_DAEMON__URL", "http://127.0.0.1:9")
            .current_dir(self.dir.path().join("repo"));
        command
    }

    fn json(&self, args: &[&str]) -> Value {
        let out = self.flowspace3(args).arg("--json").output().unwrap();
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).unwrap()
    }

    fn hook(&self, harness: &str, stdin: &str) -> Output {
        let mut child = self
            .flowspace3(&["hooks", "prompt", "--harness", harness])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(stdin.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    }
}

fn actions(envelope: &Value) -> Vec<(String, String)> {
    envelope["data"]["changes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            (
                c["harness"].as_str().unwrap().into(),
                c["action"].as_str().unwrap().into(),
            )
        })
        .collect()
}

fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
    list.iter()
        .map(|(a, b)| ((*a).into(), (*b).into()))
        .collect()
}

#[test]
fn install_status_and_uninstall_round_trip_inside_a_temp_home() {
    let b = Box_::new(&[".claude", ".codex", ".pi/agent"]);
    let installed = b.json(&["hooks", "install"]);
    assert_eq!(installed["command"], "hooks install");
    assert_eq!(
        actions(&installed),
        pairs(&[
            ("claude", "created"),
            ("codex", "created"),
            ("copilot", "skipped"),
            ("pi", "created"),
            ("omp", "skipped")
        ])
    );
    let settings: Value = serde_json::from_str(
        &std::fs::read_to_string(b.home().join(".claude/settings.json")).unwrap(),
    )
    .unwrap();
    let command = settings["hooks"]["UserPromptSubmit"][0]["hooks"][0]["command"]
        .as_str()
        .unwrap();
    assert!(
        command.ends_with("hooks prompt --harness claude"),
        "{command}"
    );
    assert!(
        std::fs::read_to_string(b.home().join(".codex/config.toml"))
            .unwrap()
            .contains("trusted_hash")
    );
    assert!(
        b.home()
            .join(".pi/agent/extensions/flowspace3.ts")
            .is_file()
    );

    let again = b.json(&["hooks", "install"]);
    assert!(
        actions(&again)
            .iter()
            .all(|(_, a)| a == "unchanged" || a == "skipped")
    );

    let status = b.json(&["hooks", "status"]);
    let claude_user = status["data"]["hooks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["harness"] == "claude" && r["scope"] == "user")
        .unwrap();
    assert_eq!(claude_user["state"], "installed");

    let removed = b.json(&["hooks", "uninstall"]);
    assert_eq!(
        actions(&removed),
        pairs(&[
            ("claude", "updated"),
            ("codex", "updated"),
            ("copilot", "unchanged"),
            ("pi", "removed"),
            ("omp", "unchanged")
        ])
    );
    assert!(!b.home().join(".pi/agent/extensions/flowspace3.ts").exists());
}

#[test]
fn the_claude_hook_hands_a_stopped_daemon_to_the_agent_and_passes_other_prompts() {
    let b = Box_::new(&[".claude"]);
    let payload = |prompt: &str| {
        serde_json::json!({"prompt": prompt, "session_id": "s", "cwd": b.dir.path().join("repo"),
                           "transcript_path": "/t.jsonl", "hook_event_name": "UserPromptSubmit"})
        .to_string()
    };
    let out = b.hook("claude", &payload("fs3 \"retry policy\" why?"));
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    let context = v["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap();
    assert!(context.contains("Search: \"retry policy\""), "{context}");
    assert!(
        context.contains("NOTE: "),
        "the failed search is reported: {context}"
    );

    let plain = b.hook("claude", &payload("just a question"));
    assert!(plain.status.success());
    assert!(plain.stdout.is_empty());

    let garbage = b.hook("codex", "not json");
    assert!(garbage.status.success());
    assert!(garbage.stdout.is_empty());
}

#[test]
fn extensions_call_the_hook_with_flags_and_get_plain_text() {
    let b = Box_::new(&[]);
    let out = b
        .flowspace3(&[
            "hooks",
            "prompt",
            "--harness",
            "pi",
            "--prompt=fs3 -help",
            "--session=s",
            "--cwd=/",
        ])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(
        text.starts_with("The user typed an `fs3` prompt but asked how it works"),
        "{text}"
    );
}
