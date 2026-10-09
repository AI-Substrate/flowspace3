//! Prompt hooks for agent harnesses: the `fs3 "…"` prompt grammar, the search
//! it runs before the agent's turn, and the installer that wires it into each
//! harness's config.
//!
//! One implementation serves every harness. Claude Code and Codex run
//! `flowspace3 hooks prompt` as a command hook and read its JSON; Copilot, pi
//! and omp run a small bundled extension that calls the same subcommand and
//! hands its plain-text output to the agent.

pub mod grammar;
pub mod install;
pub mod prompt;

use std::io::Read as _;

use serde_json::{Value, json};

use crate::DaemonClient;
pub use prompt::{Backend, Event};

/// The harnesses flowspace3 can hook.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, clap::ValueEnum)]
pub enum Harness {
    Claude,
    Codex,
    Copilot,
    Pi,
    Omp,
}

impl Harness {
    /// Every harness, in the order reports list them.
    pub const ALL: [Harness; 5] = [
        Self::Claude,
        Self::Codex,
        Self::Copilot,
        Self::Pi,
        Self::Omp,
    ];

    /// The `--harness` value.
    #[must_use]
    pub fn slug(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Copilot => "copilot",
            Self::Pi => "pi",
            Self::Omp => "omp",
        }
    }

    /// The product name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Claude => "Claude Code",
            Self::Codex => "Codex",
            Self::Copilot => "Copilot CLI",
            Self::Pi => "pi",
            Self::Omp => "omp",
        }
    }

    /// The name `conversation verify` knows this harness's chats by, or `None`
    /// when flowspace3 does not ingest them (so `-this` cannot work).
    #[must_use]
    pub fn verify_name(self) -> Option<&'static str> {
        match self {
            Self::Claude => Some("claude"),
            Self::Omp => Some("omp"),
            Self::Codex | Self::Copilot | Self::Pi => None,
        }
    }

    /// Whether the harness reads a command hook's JSON (rather than an
    /// extension's plain text).
    #[must_use]
    pub fn command_hook(self) -> bool {
        matches!(self, Self::Claude | Self::Codex)
    }
}

/// What `hooks prompt` read, from stdin JSON and/or flags (flags win).
#[derive(Debug, Default)]
pub struct Payload {
    pub prompt: Option<String>,
    pub session: Option<String>,
    pub cwd: Option<String>,
}

/// Read the harness's stdin payload. Both spellings of the session id are
/// accepted (`session_id`, Copilot's `sessionId`).
#[must_use]
pub fn read_payload(raw: &str) -> Option<Value> {
    serde_json::from_str::<Value>(raw)
        .ok()
        .filter(Value::is_object)
}

/// A Claude Code hook payload always carries `transcript_path` and never a
/// `timestamp`; Copilot CLI also runs a repo's `.claude/settings.json` hooks
/// and sends the opposite. Under Copilot the search would run, cost up to its
/// full timeout, and have its output dropped — so the claude hook steps aside.
#[must_use]
pub fn foreign_to_claude(payload: &Value) -> bool {
    payload.get("timestamp").is_some()
        || payload.get("sessionId").is_some()
        || payload.get("transcript_path").is_none()
}

fn field(payload: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|k| payload.get(*k).and_then(Value::as_str))
        .map(str::to_string)
}

/// The daemon, as the prompt hook sees it.
impl Backend for DaemonClient {
    async fn search(&self, params: Vec<(String, String)>) -> Value {
        envelope_value(DaemonClient::search(self, &params).await)
    }
    async fn status(&self) -> Value {
        envelope_value(DaemonClient::status(self, false).await)
    }
    async fn verify(&self, harness: &str, session_id: &str) -> Value {
        let params = [
            ("harness".to_string(), harness.to_string()),
            ("session_id".to_string(), session_id.to_string()),
        ];
        envelope_value(self.conversation_verify(&params).await)
    }
}

fn envelope_value(envelope: fs3_core::envelope::Envelope) -> Value {
    serde_json::to_value(envelope).unwrap_or(Value::Null)
}

/// Run the prompt hook for one harness and return what to print, if anything.
///
/// Never fails: a hook must not block a prompt. A payload it cannot read, or a
/// prompt that is not an fs3 prompt, prints nothing.
pub async fn run_prompt<B: Backend>(
    backend: &B,
    harness: Harness,
    stdin: Option<&str>,
    flags: Payload,
) -> Option<String> {
    let payload = match stdin {
        Some(raw) => read_payload(raw)?,
        None => json!({}),
    };
    if harness == Harness::Claude && stdin.is_some() && foreign_to_claude(&payload) {
        return None;
    }
    let prompt = flags.prompt.or_else(|| field(&payload, &["prompt"]))?;
    let event = Event {
        prompt,
        session_id: flags
            .session
            .or_else(|| field(&payload, &["session_id", "sessionId"]))
            .unwrap_or_default(),
        cwd: canonical(
            &flags
                .cwd
                .or_else(|| field(&payload, &["cwd"]))
                .unwrap_or_else(|| ".".into()),
        ),
        verify_harness: harness.verify_name(),
        harness: harness.name(),
    };
    let context = prompt::build_context(backend, &event).await?;
    Some(shape(harness, context))
}

/// Wrap the context in the shape this harness reads.
#[must_use]
pub fn shape(harness: Harness, context: String) -> String {
    if harness.command_hook() {
        json!({"hookSpecificOutput": {"hookEventName": "UserPromptSubmit", "additionalContext": context}})
            .to_string()
    } else {
        context
    }
}

fn canonical(path: &str) -> String {
    std::fs::canonicalize(path)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| path.to_string())
}

/// A daemon the hook could not even build a client for: every call reports why.
pub struct Unreachable(pub String);

impl Backend for Unreachable {
    async fn search(&self, _: Vec<(String, String)>) -> Value {
        self.failure()
    }
    async fn status(&self) -> Value {
        self.failure()
    }
    async fn verify(&self, _: &str, _: &str) -> Value {
        self.failure()
    }
}

impl Unreachable {
    fn failure(&self) -> Value {
        json!({"ok": false, "error": {"message": self.0}})
    }
}

/// Read all of stdin, unless it is a terminal or flags already carry the prompt.
#[must_use]
pub fn stdin_payload(flags: &Payload) -> Option<String> {
    use std::io::IsTerminal as _;
    if flags.prompt.is_some() || std::io::stdin().is_terminal() {
        return None;
    }
    let mut raw = String::new();
    std::io::stdin().read_to_string(&mut raw).ok()?;
    Some(raw)
}

/// The repo a `--scope project` install writes into: the nearest directory
/// at or above the working directory that holds `.git`, else the working
/// directory itself.
#[must_use]
pub fn project_root() -> std::path::PathBuf {
    let here = std::env::current_dir().unwrap_or_else(|_| ".".into());
    here.ancestors()
        .find(|dir| dir.join(".git").exists())
        .map_or_else(|| here.clone(), std::path::Path::to_path_buf)
}

/// The exact command that installs the hook for these harnesses.
#[must_use]
pub fn install_command(harnesses: &[Harness]) -> String {
    match harnesses {
        [one] => format!("flowspace3 hooks install --harness {}", one.slug()),
        _ => "flowspace3 hooks install".to_string(),
    }
}

/// What to do after an install or uninstall, from what changed.
fn after_change(changes: &[install::Change], installing: bool) -> String {
    if let Some(failed) = changes.iter().find(|c| c.action == "failed") {
        return format!(
            "{} was not changed: {} — fix that, then run the same command again",
            failed.path,
            failed.detail.as_deref().unwrap_or("unknown failure")
        );
    }
    let touched: Vec<&install::Change> = changes
        .iter()
        .filter(|c| matches!(c.action, "created" | "updated" | "removed"))
        .collect();
    if touched.is_empty() {
        return if installing {
            "nothing to change: type `fs3 \"<search>\" <message>` in any hooked harness \
             (`flowspace3 docs get prompt-search`)"
                .into()
        } else {
            "nothing to remove".into()
        };
    }
    let mut steps: Vec<&str> = Vec::new();
    for change in &touched {
        let step = match change.harness {
            "claude" => {
                "Claude Code reloads settings itself (open `/hooks` once if a running session misses it)"
            }
            "codex" => "restart Codex",
            _ => "start a new Copilot / pi / omp session (extensions load at startup)",
        };
        if !steps.contains(&step) {
            steps.push(step);
        }
    }
    let tail = if installing {
        "; then type `fs3 \"<search>\" <message>` (`flowspace3 docs get prompt-search`)"
    } else {
        ""
    };
    format!("{}{tail}", steps.join("; "))
}

/// `hooks install` / `hooks uninstall`.
#[must_use]
pub fn install_envelope(
    places: &install::Places,
    binary: &std::path::Path,
    harnesses: &[Harness],
    scope: install::Scope,
    all: bool,
    installing: bool,
) -> fs3_core::envelope::Envelope<install::ChangeReport> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let changes = install::apply(places, binary, harnesses, scope, all, installing, stamp);
    let next = after_change(&changes, installing);
    let verb = if installing {
        "hooks install"
    } else {
        "hooks uninstall"
    };
    fs3_core::envelope::Envelope::ok(
        verb,
        install::ChangeReport {
            binary: binary.display().to_string(),
            changes,
        },
    )
    .with_next_action(next)
}

/// `hooks status`: both scopes for every named harness.
#[must_use]
pub fn status_envelope(
    places: &install::Places,
    binary: &std::path::Path,
    harnesses: &[Harness],
) -> fs3_core::envelope::Envelope<install::StatusReport> {
    let mut hooks = Vec::new();
    for &harness in harnesses {
        for scope in [install::Scope::User, install::Scope::Project] {
            hooks.push(install::audit(places, binary, harness, scope));
        }
    }
    let wanting: Vec<Harness> = harnesses
        .iter()
        .copied()
        .filter(|&h| places.detected(h) && summary(&hooks, h).0 != "installed")
        .collect();
    let next = if wanting.is_empty() {
        "every detected harness has the hook: type `fs3 \"<search>\" <message>`".to_string()
    } else if wanting.len() == harnesses.iter().filter(|&&h| places.detected(h)).count() {
        install_command(&[])
    } else {
        wanting
            .iter()
            .map(|&h| install_command(&[h]))
            .collect::<Vec<_>>()
            .join(" && ")
    };
    fs3_core::envelope::Envelope::ok(
        "hooks status",
        install::StatusReport {
            binary: binary.display().to_string(),
            hooks,
        },
    )
    .with_next_action(next)
}

/// One harness's verdict across scopes: installed anywhere wins, then the
/// most actionable problem. Returns `(state, detail)`.
#[must_use]
pub fn summary(rows: &[install::HookStatus], harness: Harness) -> (&'static str, Option<String>) {
    let mine: Vec<&install::HookStatus> = rows
        .iter()
        .filter(|r| r.harness == harness.slug())
        .collect();
    for state in ["installed", "stale", "disabled", "conflict", "unreadable"] {
        if let Some(row) = mine.iter().find(|r| r.state == state) {
            let detail = row.detail.clone().map(|d| format!("{d} ({})", row.path));
            return (state, detail.or_else(|| Some(row.path.clone())));
        }
    }
    ("missing", None)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Never;
    impl Backend for Never {
        async fn search(&self, _: Vec<(String, String)>) -> Value {
            json!({"ok": true, "data": {"results": []}})
        }
        async fn status(&self) -> Value {
            json!({"ok": false})
        }
        async fn verify(&self, _: &str, _: &str) -> Value {
            json!({"ok": false})
        }
    }

    fn claude_payload(prompt: &str) -> String {
        json!({"prompt": prompt, "session_id": "s", "cwd": "/", "transcript_path": "/t.jsonl",
               "hook_event_name": "UserPromptSubmit"})
        .to_string()
    }

    #[tokio::test]
    async fn claude_and_codex_get_hook_json() {
        for harness in [Harness::Claude, Harness::Codex] {
            let out = run_prompt(
                &Never,
                harness,
                Some(&claude_payload("fs3 \"x\"")),
                Payload::default(),
            )
            .await
            .unwrap();
            let v: Value = serde_json::from_str(&out).unwrap();
            assert_eq!(v["hookSpecificOutput"]["hookEventName"], "UserPromptSubmit");
            assert!(
                v["hookSpecificOutput"]["additionalContext"]
                    .as_str()
                    .unwrap()
                    .contains("Search: \"x\"")
            );
        }
    }

    #[tokio::test]
    async fn extensions_get_plain_text_from_flags() {
        let flags = Payload {
            prompt: Some("fs3 \"x\"".into()),
            session: Some("s".into()),
            cwd: Some("/".into()),
        };
        let out = run_prompt(&Never, Harness::Pi, None, flags).await.unwrap();
        assert!(out.starts_with("flowspace3 search results"));
    }

    #[tokio::test]
    async fn copilot_payload_under_the_claude_hook_steps_aside() {
        let copilot = json!({"prompt": "fs3 \"x\"", "sessionId": "s", "cwd": "/", "timestamp": 1})
            .to_string();
        assert_eq!(
            run_prompt(&Never, Harness::Claude, Some(&copilot), Payload::default()).await,
            None
        );
        let pascal = json!({"prompt": "fs3 \"x\"", "session_id": "s", "cwd": "/",
                            "hook_event_name": "UserPromptSubmit", "timestamp": "2026-10-09T00:00:00Z"})
        .to_string();
        assert_eq!(
            run_prompt(&Never, Harness::Claude, Some(&pascal), Payload::default()).await,
            None
        );
    }

    #[tokio::test]
    async fn non_fs3_and_garbage_print_nothing() {
        assert_eq!(
            run_prompt(
                &Never,
                Harness::Claude,
                Some(&claude_payload("hi")),
                Payload::default()
            )
            .await,
            None
        );
        assert_eq!(
            run_prompt(&Never, Harness::Codex, Some("not json"), Payload::default()).await,
            None
        );
    }
}
