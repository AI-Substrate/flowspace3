//! `flowspace3 hooks install | status | uninstall`: wire the prompt hook into
//! each harness's own config.
//!
//! Every write merges: another tool's hooks (git-ai's included) are never
//! touched, a file flowspace3 did not write is never overwritten, a changed
//! file is backed up first, and an install that is already current writes
//! nothing. Paths are explicit ([`Places`]) so tests never touch the real `~`.
//!
//! Config locations, the merge rules and Codex's trust hash follow git-ai's
//! installers (Apache-2.0, `src/mdm/agents/*`; see NOTICE).

use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use toml_edit::{ArrayOfTables, DocumentMut, Item, Table};

use super::Harness;

/// The first line of every extension file flowspace3 writes; a file without
/// it is someone else's and is never overwritten or deleted.
pub const MARKER: &str = "Written by `flowspace3 hooks install`";
/// Claude Code's per-hook timeout, seconds. The hook bounds itself at 25s.
const CLAUDE_TIMEOUT: u64 = 40;
/// Codex's event key, and its snake-case form in trust-state keys.
const CODEX_EVENT: &str = "UserPromptSubmit";
const CODEX_EVENT_SNAKE: &str = "user_prompt_submit";
/// The handler timeout Codex hashes when none is written (its default).
const CODEX_DEFAULT_TIMEOUT: i64 = 600;
/// The Python hook this binary replaces; a registration of it is stale.
const LEGACY_SCRIPT: &str = "fs3-prompt-search.py";

const COPILOT_EXTENSION: &str = include_str!("extensions/copilot.mjs");
const PI_EXTENSION: &str = include_str!("extensions/pi.ts");

/// Which config layer to write.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    /// Every repo, for this user (the default).
    User,
    /// This repo only.
    Project,
}

/// Where each harness keeps its config.
#[derive(Debug, Clone)]
pub struct Places {
    pub home: PathBuf,
    pub project: PathBuf,
    pub codex_home: PathBuf,
    pub copilot_home: PathBuf,
}

impl Places {
    /// The real locations: `$HOME`, `$CODEX_HOME`, `$COPILOT_HOME`.
    #[must_use]
    pub fn from_env(project: PathBuf) -> Option<Self> {
        let home = PathBuf::from(std::env::var_os("HOME")?);
        let env_dir = |name: &str| {
            std::env::var_os(name)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        };
        Some(Self {
            codex_home: env_dir("CODEX_HOME").unwrap_or_else(|| home.join(".codex")),
            copilot_home: env_dir("COPILOT_HOME").unwrap_or_else(|| home.join(".copilot")),
            home,
            project,
        })
    }

    /// The directory whose presence says this user has the harness.
    #[must_use]
    pub fn root(&self, harness: Harness) -> PathBuf {
        match harness {
            Harness::Claude => self.home.join(".claude"),
            Harness::Codex => self.codex_home.clone(),
            Harness::Copilot => self.copilot_home.clone(),
            Harness::Pi => self.home.join(".pi/agent"),
            Harness::Omp => self.home.join(".omp/agent"),
        }
    }

    /// Whether this user has the harness.
    #[must_use]
    pub fn detected(&self, harness: Harness) -> bool {
        self.root(harness).is_dir()
    }

    /// The file the hook lives in.
    #[must_use]
    pub fn target(&self, harness: Harness, scope: Scope) -> PathBuf {
        let p = &self.project;
        match (harness, scope) {
            (Harness::Claude, Scope::User) => self.home.join(".claude/settings.json"),
            (Harness::Claude, Scope::Project) => p.join(".claude/settings.local.json"),
            (Harness::Codex, Scope::User) => self.codex_home.join("config.toml"),
            (Harness::Codex, Scope::Project) => p.join(".codex/config.toml"),
            (Harness::Copilot, Scope::User) => self
                .copilot_home
                .join("extensions/flowspace3/extension.mjs"),
            (Harness::Copilot, Scope::Project) => {
                p.join(".github/extensions/flowspace3/extension.mjs")
            }
            (Harness::Pi, Scope::User) => self.home.join(".pi/agent/extensions/flowspace3.ts"),
            (Harness::Pi, Scope::Project) => p.join(".pi/extensions/flowspace3.ts"),
            (Harness::Omp, Scope::User) => self.home.join(".omp/agent/extensions/flowspace3.ts"),
            (Harness::Omp, Scope::Project) => p.join(".omp/extensions/flowspace3.ts"),
        }
    }
}

/// The binary the hooks should run: the `flowspace3` on `PATH` when it is this
/// very binary (a stable path that survives upgrades), else this executable.
#[must_use]
pub fn resolve_binary() -> PathBuf {
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("flowspace3"));
    let exe_real = std::fs::canonicalize(&exe).unwrap_or_else(|_| exe.clone());
    let on_path = std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|dir| dir.join("flowspace3"))
            .find(|candidate| candidate.is_file())
    });
    match on_path {
        Some(candidate) if std::fs::canonicalize(&candidate).ok().as_ref() == Some(&exe_real) => {
            candidate
        }
        _ => exe_real,
    }
}

fn shell_quote(path: &Path) -> String {
    let text = path.to_string_lossy();
    if text
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "/._-+".contains(c))
    {
        text.into_owned()
    } else {
        format!("'{}'", text.replace('\'', r"'\''"))
    }
}

/// The command a command-hook harness runs.
#[must_use]
pub fn hook_command(binary: &Path, harness: Harness) -> String {
    format!(
        "{} hooks prompt --harness {}",
        shell_quote(binary),
        harness.slug()
    )
}

/// Whether a registered command is ours for this harness, current or not.
fn is_ours(command: &str, harness: Harness) -> bool {
    command.contains(&format!("hooks prompt --harness {}", harness.slug()))
        || (harness == Harness::Claude && command.contains(LEGACY_SCRIPT))
}

/// The extension file for a harness that loads extensions.
#[must_use]
pub fn extension_source(binary: &Path, harness: Harness) -> String {
    let template = if harness == Harness::Copilot {
        COPILOT_EXTENSION
    } else {
        PI_EXTENSION
    };
    let literal = serde_json::to_string(&binary.to_string_lossy())
        .unwrap_or_else(|_| "\"flowspace3\"".into());
    template
        .replace("{{MARKER}}", MARKER)
        .replace("{{HARNESS}}", harness.slug())
        .replace("{{NAME}}", harness.name())
        .replace("{{FS3}}", &literal)
        .replace("{{VERSION}}", env!("CARGO_PKG_VERSION"))
}

/// What one harness's hook looks like at one scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HookStatus {
    pub harness: &'static str,
    pub scope: Scope,
    /// Whether this user has the harness at all.
    pub detected: bool,
    pub path: String,
    /// `installed`, `missing`, `stale`, `disabled`, `conflict` or `unreadable`.
    pub state: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// One file `install`/`uninstall` looked at.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Change {
    pub harness: &'static str,
    pub scope: Scope,
    pub path: String,
    /// `created`, `updated`, `removed`, `unchanged`, `skipped` or `failed`.
    pub action: &'static str,
    /// Where the previous content was saved, when a file was changed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// What `hooks install` / `hooks uninstall` answer with.
#[derive(Debug, Clone, Serialize)]
pub struct ChangeReport {
    /// The binary the hooks run.
    pub binary: String,
    pub changes: Vec<Change>,
}

/// What `hooks status` answers with.
#[derive(Debug, Clone, Serialize)]
pub struct StatusReport {
    pub binary: String,
    pub hooks: Vec<HookStatus>,
}

type Audit = (&'static str, Option<String>);

/// Read one harness's hook state without writing.
#[must_use]
pub fn audit(places: &Places, binary: &Path, harness: Harness, scope: Scope) -> HookStatus {
    let path = places.target(harness, scope);
    let (state, detail) = match std::fs::read_to_string(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => ("missing", None),
        Err(e) => ("unreadable", Some(e.to_string())),
        Ok(text) => audit_text(&text, binary, harness, &path),
    };
    HookStatus {
        harness: harness.slug(),
        scope,
        detected: places.detected(harness),
        path: path.display().to_string(),
        state,
        detail,
    }
}

fn audit_text(text: &str, binary: &Path, harness: Harness, path: &Path) -> Audit {
    let desired = hook_command(binary, harness);
    match harness {
        Harness::Claude => match claude_commands(text) {
            Err(why) => ("unreadable", Some(why)),
            Ok(found) => audit_commands(&found, &desired, harness),
        },
        Harness::Codex => match text.parse::<DocumentMut>() {
            Err(e) => ("unreadable", Some(format!("not valid TOML: {e}"))),
            Ok(doc) => {
                let key = path.display().to_string();
                let found: Vec<String> = codex_handlers(&doc)
                    .into_iter()
                    .filter(|h| h.ours)
                    .map(|h| h.command)
                    .collect();
                let (state, detail) = audit_commands(&found, &desired, harness);
                if state != "installed" {
                    return (state, detail);
                }
                if doc
                    .get("features")
                    .and_then(|f| f.get("hooks"))
                    .and_then(Item::as_bool)
                    == Some(false)
                {
                    return (
                        "disabled",
                        Some("Codex hooks are turned off: set `[features] hooks = true`".into()),
                    );
                }
                let position = codex_handlers(&doc).into_iter().find(|h| h.ours);
                let trusted = position.is_some_and(|h| {
                    codex_state_hash(&doc, &state_key(&key, h.group, h.handler))
                        == Some(trust_hash(&desired))
                });
                if trusted {
                    ("installed", None)
                } else {
                    (
                        "stale",
                        Some(
                            "registered but not trusted: Codex will skip it until approved".into(),
                        ),
                    )
                }
            }
        },
        Harness::Copilot | Harness::Pi | Harness::Omp => {
            if text == extension_source(binary, harness) {
                ("installed", None)
            } else if text.contains(MARKER) {
                (
                    "stale",
                    Some("written by another flowspace3 version or binary path".into()),
                )
            } else {
                (
                    "conflict",
                    Some("a file flowspace3 did not write is in the way".into()),
                )
            }
        }
    }
}

fn audit_commands(found: &[String], desired: &str, harness: Harness) -> Audit {
    match found {
        [] => ("missing", None),
        [only] if only == desired => ("installed", None),
        [only] if harness == Harness::Claude && only.contains(LEGACY_SCRIPT) => (
            "stale",
            Some(format!("registered as the old Python hook ({only})")),
        ),
        [only] => (
            "stale",
            Some(format!("registered for another binary: {only}")),
        ),
        many => ("stale", Some(format!("registered {} times", many.len()))),
    }
}

/// Install (or, with `install = false`, remove) the hook for these harnesses.
///
/// `all` names a harness set chosen by `--harness all`: harnesses this user
/// does not have are skipped rather than created.
#[must_use]
pub fn apply(
    places: &Places,
    binary: &Path,
    harnesses: &[Harness],
    scope: Scope,
    all: bool,
    install: bool,
    stamp: u64,
) -> Vec<Change> {
    harnesses
        .iter()
        .map(|&harness| {
            let path = places.target(harness, scope);
            let change = |action, backup, detail| Change {
                harness: harness.slug(),
                scope,
                path: path.display().to_string(),
                action,
                backup,
                detail,
            };
            if install && all && !places.detected(harness) {
                return change(
                    "skipped",
                    None,
                    Some(format!(
                        "{} not detected ({} absent)",
                        harness.name(),
                        places.root(harness).display()
                    )),
                );
            }
            match apply_one(&path, binary, harness, install, stamp) {
                Ok((action, backup)) => {
                    change(action, backup.map(|b| b.display().to_string()), None)
                }
                Err(why) => change("failed", None, Some(why)),
            }
        })
        .collect()
}

fn apply_one(
    path: &Path,
    binary: &Path,
    harness: Harness,
    install: bool,
    stamp: u64,
) -> Result<(&'static str, Option<PathBuf>), String> {
    let existing = match std::fs::read_to_string(path) {
        Ok(text) => Some(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(format!("cannot read {}: {e}", path.display())),
    };
    let desired = install.then(|| hook_command(binary, harness));
    let next: Option<String> = match harness {
        Harness::Claude => Some(claude_rewrite(
            existing.as_deref(),
            desired.as_deref(),
            harness,
        )?),
        Harness::Codex => Some(codex_rewrite(
            existing.as_deref(),
            desired.as_deref(),
            &path.display().to_string(),
        )?),
        Harness::Copilot | Harness::Pi | Harness::Omp => {
            if let Some(text) = &existing
                && !text.contains(MARKER)
            {
                return Err(format!(
                    "{} exists and was not written by flowspace3; move it aside and run again",
                    path.display()
                ));
            }
            install.then(|| extension_source(binary, harness))
        }
    };
    let action = match (&existing, &next) {
        (None, None) => return Ok(("unchanged", None)),
        (Some(old), Some(new)) if old == new => return Ok(("unchanged", None)),
        (None, Some(_)) if !install => return Ok(("unchanged", None)),
        (None, Some(_)) => "created",
        (Some(_), Some(_)) => "updated",
        (Some(_), None) => "removed",
    };
    // Shared configs are backed up; an extension file is wholly ours and
    // regenerated exactly, so a copy would only litter the extensions folder.
    let backup = match &existing {
        Some(old) if harness.command_hook() => {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let backup = path.with_file_name(format!("{name}.fs3-backup-{stamp}"));
            std::fs::write(&backup, old)
                .map_err(|e| format!("cannot back up to {}: {e}", backup.display()))?;
            Some(backup)
        }
        _ => None,
    };
    match next {
        Some(text) => write_atomic(path, &text)?,
        None => {
            std::fs::remove_file(path)
                .map_err(|e| format!("cannot remove {}: {e}", path.display()))?;
            if let Some(dir) = path.parent()
                && dir.file_name().is_some_and(|n| n == "flowspace3")
            {
                let _ = std::fs::remove_dir(dir); // only when empty
            }
        }
    }
    Ok((action, backup))
}

fn write_atomic(path: &Path, text: &str) -> Result<(), String> {
    let dir = path
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", path.display()))?;
    std::fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let temp = path.with_extension("fs3-tmp");
    std::fs::write(&temp, text).map_err(|e| format!("cannot write {}: {e}", temp.display()))?;
    std::fs::rename(&temp, path).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

// ---- Claude Code: settings.json -------------------------------------------

fn claude_root(text: Option<&str>) -> Result<Value, String> {
    match text {
        Some(t) if !t.trim().is_empty() => {
            let root: Value =
                serde_json::from_str(t).map_err(|e| format!("not valid JSON: {e}"))?;
            if root.is_object() {
                Ok(root)
            } else {
                Err("the settings root is not a JSON object".into())
            }
        }
        _ => Ok(json!({})),
    }
}

fn handler_command(handler: &Value) -> &str {
    handler.get("command").and_then(Value::as_str).unwrap_or("")
}

fn claude_commands(text: &str) -> Result<Vec<String>, String> {
    let root = claude_root(Some(text))?;
    Ok(root
        .pointer("/hooks/UserPromptSubmit")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|group| group.get("hooks").and_then(Value::as_array))
        .flatten()
        .map(handler_command)
        .filter(|c| is_ours(c, Harness::Claude))
        .map(str::to_string)
        .collect())
}

fn claude_rewrite(
    text: Option<&str>,
    desired: Option<&str>,
    harness: Harness,
) -> Result<String, String> {
    let mut root = claude_root(text)?;
    let fresh =
        |command: &str| json!({"type": "command", "command": command, "timeout": CLAUDE_TIMEOUT});
    let obj = root
        .as_object_mut()
        .ok_or("the settings root is not a JSON object")?;
    let hooks = obj.entry("hooks").or_insert_with(|| json!({}));
    let hooks = hooks
        .as_object_mut()
        .ok_or("`hooks` is not a JSON object")?;
    let groups = hooks.entry("UserPromptSubmit").or_insert_with(|| json!([]));
    let groups = groups
        .as_array_mut()
        .ok_or("`hooks.UserPromptSubmit` is not a JSON array")?;

    let mut placed = false;
    let mut emptied = Vec::new();
    for (index, group) in groups.iter_mut().enumerate() {
        let Some(handlers) = group.get_mut("hooks").and_then(Value::as_array_mut) else {
            continue;
        };
        let before = handlers.len();
        let mut kept = Vec::with_capacity(before);
        for handler in handlers.drain(..) {
            if !is_ours(handler_command(&handler), harness) {
                kept.push(handler);
            } else if let Some(command) = desired
                && !placed
            {
                // Replace in place, keeping a still-current entry byte for byte.
                kept.push(if handler_command(&handler) == command {
                    handler
                } else {
                    fresh(command)
                });
                placed = true;
            }
        }
        if before > 0 && kept.is_empty() {
            emptied.push(index);
        }
        *handlers = kept;
    }
    for index in emptied.into_iter().rev() {
        groups.remove(index);
    }
    if let Some(command) = desired
        && !placed
    {
        groups.push(json!({"hooks": [fresh(command)]}));
    }
    if groups.is_empty() {
        hooks.remove("UserPromptSubmit");
    }
    if hooks.is_empty() {
        obj.remove("hooks");
    }
    if text.is_some_and(|t| claude_root(Some(t)).ok().as_ref() == Some(&root)) {
        return Ok(text.unwrap_or_default().to_string()); // no semantic change: keep the bytes
    }
    let shape = text.and_then(|t| serde_json::from_str::<Keys>(t).ok());
    serde_json::to_string_pretty(&Shaped(&root, shape.as_ref()))
        .map(|s| s + "\n")
        .map_err(|e| e.to_string())
}

/// The key order of a JSON document, at every depth. `serde_json::Value`
/// sorts keys (the workspace must not turn on `preserve_order`: envelopes are
/// byte-pinned), so a rewritten settings file borrows its order from here.
enum Keys {
    Object(Vec<(String, Keys)>),
    Array(Vec<Keys>),
    Leaf,
}

impl<'de> serde::Deserialize<'de> for Keys {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visit;
        impl<'de> serde::de::Visitor<'de> for Visit {
            type Value = Keys;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("any JSON value")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> Result<Keys, A::Error> {
                let mut keys = Vec::new();
                while let Some((key, inner)) = map.next_entry::<String, Keys>()? {
                    keys.push((key, inner));
                }
                Ok(Keys::Object(keys))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<Keys, A::Error> {
                let mut items = Vec::new();
                while let Some(inner) = seq.next_element::<Keys>()? {
                    items.push(inner);
                }
                Ok(Keys::Array(items))
            }
            fn visit_bool<E>(self, _: bool) -> Result<Keys, E> {
                Ok(Keys::Leaf)
            }
            fn visit_i64<E>(self, _: i64) -> Result<Keys, E> {
                Ok(Keys::Leaf)
            }
            fn visit_u64<E>(self, _: u64) -> Result<Keys, E> {
                Ok(Keys::Leaf)
            }
            fn visit_f64<E>(self, _: f64) -> Result<Keys, E> {
                Ok(Keys::Leaf)
            }
            fn visit_str<E>(self, _: &str) -> Result<Keys, E> {
                Ok(Keys::Leaf)
            }
            fn visit_unit<E>(self) -> Result<Keys, E> {
                Ok(Keys::Leaf)
            }
        }
        deserializer.deserialize_any(Visit)
    }
}

/// A value serialised in the key order `Keys` recorded; keys it has never
/// seen follow, sorted.
struct Shaped<'a>(&'a Value, Option<&'a Keys>);

impl serde::Serialize for Shaped<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::{SerializeMap as _, SerializeSeq as _};
        match (self.0, self.1) {
            (Value::Object(map), shape) => {
                let known: &[(String, Keys)] = match shape {
                    Some(Keys::Object(keys)) => keys,
                    _ => &[],
                };
                let mut out = serializer.serialize_map(Some(map.len()))?;
                for (key, inner) in known {
                    if let Some(value) = map.get(key) {
                        out.serialize_entry(key, &Shaped(value, Some(inner)))?;
                    }
                }
                for (key, value) in map {
                    if !known.iter().any(|(k, _)| k == key) {
                        out.serialize_entry(key, &Shaped(value, None))?;
                    }
                }
                out.end()
            }
            (Value::Array(items), shape) => {
                let known: &[Keys] = match shape {
                    Some(Keys::Array(items)) => items,
                    _ => &[],
                };
                let mut out = serializer.serialize_seq(Some(items.len()))?;
                for (index, item) in items.iter().enumerate() {
                    out.serialize_element(&Shaped(item, known.get(index)))?;
                }
                out.end()
            }
            (value, _) => value.serialize(serializer),
        }
    }
}

// ---- Codex: config.toml ---------------------------------------------------

#[derive(Debug, Clone)]
struct CodexHandler {
    group: usize,
    handler: usize,
    command: String,
    ours: bool,
}

fn codex_groups(doc: &DocumentMut) -> Option<&ArrayOfTables> {
    doc.get("hooks")?.get(CODEX_EVENT)?.as_array_of_tables()
}

fn codex_handlers(doc: &DocumentMut) -> Vec<CodexHandler> {
    let mut found = Vec::new();
    for (group, table) in codex_groups(doc)
        .into_iter()
        .flat_map(ArrayOfTables::iter)
        .enumerate()
    {
        let Some(handlers) = table.get("hooks").and_then(Item::as_array_of_tables) else {
            continue;
        };
        for (handler, entry) in handlers.iter().enumerate() {
            let command = entry
                .get("command")
                .and_then(Item::as_str)
                .unwrap_or("")
                .to_string();
            let ours = is_ours(&command, Harness::Codex);
            found.push(CodexHandler {
                group,
                handler,
                command,
                ours,
            });
        }
    }
    found
}

fn state_key(config_path: &str, group: usize, handler: usize) -> String {
    format!("{config_path}:{CODEX_EVENT_SNAKE}:{group}:{handler}")
}

fn codex_state_hash(doc: &DocumentMut, key: &str) -> Option<String> {
    doc.get("hooks")?
        .get("state")?
        .get(key)?
        .get("trusted_hash")?
        .as_str()
        .map(str::to_string)
}

/// Codex's approval hash for one command handler: sha256 of the canonical
/// (sorted-key, compact) JSON of the normalised hook identity.
#[must_use]
pub fn trust_hash(command: &str) -> String {
    let identity = json!({
        "event_name": CODEX_EVENT_SNAKE,
        "hooks": [{"async": false, "command": command, "timeout": CODEX_DEFAULT_TIMEOUT, "type": "command"}],
    });
    // serde_json's map sorts its keys: exactly the canonical form Codex hashes.
    let bytes = serde_json::to_vec(&identity).unwrap_or_default();
    let digest = Sha256::digest(&bytes);
    format!(
        "sha256:{}",
        digest
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    )
}

fn codex_rewrite(
    text: Option<&str>,
    desired: Option<&str>,
    config_path: &str,
) -> Result<String, String> {
    let original = text.unwrap_or("");
    let mut doc: DocumentMut = original
        .parse()
        .map_err(|e| format!("not valid TOML: {e}"))?;
    let before = codex_handlers(&doc);
    if desired.is_none() && !before.iter().any(|h| h.ours) {
        return Ok(original.to_string());
    }

    if doc.get("hooks").is_none() {
        let mut table = Table::new();
        table.set_implicit(true);
        doc.insert("hooks", Item::Table(table));
    }
    let hooks = doc["hooks"]
        .as_table_mut()
        .ok_or("`hooks` is not a TOML table")?;
    if hooks.get(CODEX_EVENT).is_none() && desired.is_some() {
        hooks.insert(CODEX_EVENT, Item::ArrayOfTables(ArrayOfTables::new()));
    }
    if let Some(item) = hooks.get_mut(CODEX_EVENT) {
        let groups = item
            .as_array_of_tables_mut()
            .ok_or("`hooks.UserPromptSubmit` is not an array of tables; edit it by hand")?;
        let mut placed = false;
        let mut emptied = Vec::new();
        for (index, group) in groups.iter_mut().enumerate() {
            let Some(handlers) = group
                .get_mut("hooks")
                .and_then(Item::as_array_of_tables_mut)
            else {
                continue;
            };
            let before_len = handlers.len();
            let mut drop = Vec::new();
            for (h, entry) in handlers.iter_mut().enumerate() {
                let command = entry
                    .get("command")
                    .and_then(Item::as_str)
                    .unwrap_or("")
                    .to_string();
                if !is_ours(&command, Harness::Codex) {
                    continue;
                }
                match desired {
                    Some(want) if !placed => {
                        if command != want {
                            entry.insert("command", toml_edit::value(want));
                        }
                        placed = true;
                    }
                    _ => drop.push(h),
                }
            }
            for h in drop.into_iter().rev() {
                handlers.remove(h);
            }
            if before_len > 0 && handlers.is_empty() {
                emptied.push(index);
            }
        }
        for index in emptied.into_iter().rev() {
            groups.remove(index);
        }
        if let Some(want) = desired
            && !placed
        {
            let mut handler = Table::new();
            handler.insert("command", toml_edit::value(want));
            handler.insert("type", toml_edit::value("command"));
            let mut handlers = ArrayOfTables::new();
            handlers.push(handler);
            let mut group = Table::new();
            group.insert("hooks", Item::ArrayOfTables(handlers));
            groups.push(group);
        }
        if groups.is_empty() {
            hooks.remove(CODEX_EVENT);
        }
    }

    // Trust state is keyed by position. Move other tools' entries with their
    // handlers, drop ours, and trust our new one.
    let after = codex_handlers(&doc);
    let old_theirs: Vec<&CodexHandler> = before.iter().filter(|h| !h.ours).collect();
    let new_theirs: Vec<&CodexHandler> = after.iter().filter(|h| !h.ours).collect();
    let hooks = doc["hooks"]
        .as_table_mut()
        .ok_or("`hooks` is not a TOML table")?;
    if hooks.get("state").is_none() && desired.is_some() {
        let mut table = Table::new();
        table.set_implicit(true);
        hooks.insert("state", Item::Table(table));
    }
    if let Some(state) = hooks.get_mut("state").and_then(Item::as_table_mut) {
        let mut moved = Vec::new();
        for (old, new) in old_theirs.iter().zip(&new_theirs) {
            let (from, to) = (
                state_key(config_path, old.group, old.handler),
                state_key(config_path, new.group, new.handler),
            );
            if from != to
                && let Some(entry) = state.remove(&from)
            {
                moved.push((to, entry));
            }
        }
        for ours in before.iter().filter(|h| h.ours) {
            state.remove(&state_key(config_path, ours.group, ours.handler));
        }
        for (key, entry) in moved {
            state.insert(&key, entry);
        }
        if let (Some(want), Some(ours)) = (desired, after.iter().find(|h| h.ours)) {
            let mut entry = Table::new();
            entry.insert("enabled", toml_edit::value(true));
            entry.insert("trusted_hash", toml_edit::value(trust_hash(want)));
            state.insert(
                &state_key(config_path, ours.group, ours.handler),
                Item::Table(entry),
            );
        }
        if state.is_empty() {
            hooks.remove("state");
        }
    }
    if hooks.is_empty() {
        doc.remove("hooks");
    }
    Ok(doc.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Sandbox {
        _dir: tempfile::TempDir,
        places: Places,
        binary: PathBuf,
    }

    fn sandbox() -> Sandbox {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let places = Places {
            codex_home: home.join(".codex"),
            copilot_home: home.join(".copilot"),
            project: dir.path().join("repo"),
            home,
        };
        for h in Harness::ALL {
            std::fs::create_dir_all(places.root(h)).unwrap();
        }
        Sandbox {
            binary: dir.path().join("bin/flowspace3"),
            places,
            _dir: dir,
        }
    }

    fn install(s: &Sandbox, harnesses: &[Harness]) -> Vec<Change> {
        apply(&s.places, &s.binary, harnesses, Scope::User, false, true, 1)
    }

    fn actions(changes: &[Change]) -> Vec<&str> {
        changes.iter().map(|c| c.action).collect()
    }

    #[test]
    fn trust_hash_reproduces_an_approved_codex_entry() {
        // git-ai's checkpoint hook, approved in a real ~/.codex/config.toml under
        // post_tool_use. Our recipe differs only in the event name, so check the
        // recipe itself against that known-good hash.
        let identity = json!({"event_name": "post_tool_use", "hooks": [{"async": false,
            "command": "/Users/jordanknight/.git-ai/bin/git-ai checkpoint codex --hook-input stdin",
            "timeout": 600, "type": "command"}]});
        let digest = Sha256::digest(serde_json::to_vec(&identity).unwrap());
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(
            hex,
            "ee86881337181fed4002b031e4374990c7f09aa4e967aab48241636a25f9cfe6"
        );
    }

    #[test]
    fn claude_install_merges_is_idempotent_and_uninstalls_cleanly() {
        let s = sandbox();
        let path = s.places.target(Harness::Claude, Scope::User);
        let theirs = r#"{
  "model": "opus",
  "hooks": {
    "UserPromptSubmit": [{"hooks": [{"type": "command", "command": "other-tool"}]}],
    "PostToolUse": [{"matcher": "*", "hooks": [{"type": "command", "command": "git-ai checkpoint"}]}]
  }
}"#;
        std::fs::write(&path, theirs).unwrap();
        let first = install(&s, &[Harness::Claude]);
        assert_eq!(actions(&first), ["updated"]);
        assert!(
            first[0]
                .backup
                .as_ref()
                .is_some_and(|b| std::fs::read_to_string(b).unwrap() == theirs)
        );
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let groups = v["hooks"]["UserPromptSubmit"].as_array().unwrap();
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0]["hooks"][0]["command"], "other-tool");
        assert_eq!(
            groups[1]["hooks"][0]["command"],
            hook_command(&s.binary, Harness::Claude)
        );
        assert_eq!(
            v["hooks"]["PostToolUse"][0]["hooks"][0]["command"],
            "git-ai checkpoint"
        );
        assert_eq!(v["model"], "opus");
        let written = std::fs::read_to_string(&path).unwrap();
        let at = |needle: &str| written.find(needle).unwrap();
        assert!(
            at("\"model\"") < at("\"hooks\""),
            "top-level key order is kept"
        );
        assert!(
            at("\"UserPromptSubmit\"") < at("\"PostToolUse\""),
            "nested key order is kept"
        );
        assert!(
            at("\"matcher\"") < at("\"git-ai checkpoint\""),
            "inner key order is kept"
        );
        assert_eq!(
            audit(&s.places, &s.binary, Harness::Claude, Scope::User).state,
            "installed"
        );

        assert_eq!(actions(&install(&s, &[Harness::Claude])), ["unchanged"]);

        let removed = apply(
            &s.places,
            &s.binary,
            &[Harness::Claude],
            Scope::User,
            false,
            false,
            2,
        );
        assert_eq!(actions(&removed), ["updated"]);
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(v["hooks"]["UserPromptSubmit"].as_array().unwrap().len(), 1);
        assert_eq!(
            audit(&s.places, &s.binary, Harness::Claude, Scope::User).state,
            "missing"
        );
        let again = apply(
            &s.places,
            &s.binary,
            &[Harness::Claude],
            Scope::User,
            false,
            false,
            3,
        );
        assert_eq!(actions(&again), ["unchanged"]);
    }

    #[test]
    fn the_python_hook_and_old_binaries_are_stale_and_replaced_in_place() {
        let s = sandbox();
        let path = s.places.target(Harness::Claude, Scope::User);
        std::fs::write(
            &path,
            r#"{"hooks":{"UserPromptSubmit":[{"hooks":[
                {"type":"command","command":"$HOME/.claude/hooks/fs3-prompt-search.py","timeout":40},
                {"type":"command","command":"keep-me"}]}]}}"#,
        )
        .unwrap();
        let status = audit(&s.places, &s.binary, Harness::Claude, Scope::User);
        assert_eq!(status.state, "stale");
        assert!(status.detail.unwrap().contains("old Python hook"));
        let _ = install(&s, &[Harness::Claude]);
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let handlers = v["hooks"]["UserPromptSubmit"][0]["hooks"]
            .as_array()
            .unwrap();
        assert_eq!(handlers.len(), 2);
        assert_eq!(
            handlers[0]["command"],
            hook_command(&s.binary, Harness::Claude)
        );
        assert_eq!(handlers[1]["command"], "keep-me");

        let moved = PathBuf::from("/opt/new/flowspace3");
        let status = audit(&s.places, &moved, Harness::Claude, Scope::User);
        assert_eq!(status.state, "stale");
        assert!(status.detail.unwrap().contains("another binary"));
    }

    const CODEX_WITH_GIT_AI: &str = r#"model = "gpt"

[features]
hooks = true

# git-ai's hooks
[[hooks.UserPromptSubmit]]

[[hooks.UserPromptSubmit.hooks]]
command = "git-ai prompt"
type = "command"

[[hooks.PostToolUse]]

[[hooks.PostToolUse.hooks]]
command = "git-ai checkpoint"
type = "command"

[hooks.state."CONFIG:user_prompt_submit:0:0"]
enabled = true
trusted_hash = "sha256:theirs"
"#;

    #[test]
    fn codex_install_keeps_comments_and_other_hooks_and_trusts_ours() {
        let s = sandbox();
        let path = s.places.target(Harness::Codex, Scope::User);
        let key = path.display().to_string();
        std::fs::write(&path, CODEX_WITH_GIT_AI.replace("CONFIG", &key)).unwrap();
        assert_eq!(actions(&install(&s, &[Harness::Codex])), ["updated"]);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("# git-ai's hooks") && text.contains("model = \"gpt\""));
        let doc: DocumentMut = text.parse().unwrap();
        let handlers = codex_handlers(&doc);
        assert_eq!(handlers.len(), 2);
        assert_eq!((handlers[1].group, handlers[1].handler), (1, 0));
        let ours = hook_command(&s.binary, Harness::Codex);
        assert_eq!(
            codex_state_hash(&doc, &state_key(&key, 1, 0)),
            Some(trust_hash(&ours))
        );
        assert_eq!(
            codex_state_hash(&doc, &state_key(&key, 0, 0)).as_deref(),
            Some("sha256:theirs")
        );
        assert_eq!(
            audit(&s.places, &s.binary, Harness::Codex, Scope::User).state,
            "installed"
        );
        assert_eq!(actions(&install(&s, &[Harness::Codex])), ["unchanged"]);
    }

    #[test]
    fn codex_uninstall_moves_other_tools_trust_with_their_handlers() {
        let s = sandbox();
        let path = s.places.target(Harness::Codex, Scope::User);
        let key = path.display().to_string();
        let _ = install(&s, &[Harness::Codex]); // ours is group 0 on a fresh file
        let mut text = std::fs::read_to_string(&path).unwrap();
        text.push_str(&format!(
            "\n[[hooks.UserPromptSubmit]]\n\n[[hooks.UserPromptSubmit.hooks]]\ncommand = \"later-tool\"\ntype = \"command\"\n\n[hooks.state.\"{key}:user_prompt_submit:1:0\"]\nenabled = true\ntrusted_hash = \"sha256:later\"\n"
        ));
        std::fs::write(&path, &text).unwrap();
        let _ = apply(
            &s.places,
            &s.binary,
            &[Harness::Codex],
            Scope::User,
            false,
            false,
            2,
        );
        let doc: DocumentMut = std::fs::read_to_string(&path).unwrap().parse().unwrap();
        let handlers = codex_handlers(&doc);
        assert_eq!(handlers.len(), 1);
        assert_eq!(handlers[0].command, "later-tool");
        assert_eq!(
            codex_state_hash(&doc, &state_key(&key, 0, 0)).as_deref(),
            Some("sha256:later")
        );
        assert_eq!(codex_state_hash(&doc, &state_key(&key, 1, 0)), None);
    }

    #[test]
    fn codex_untrusted_or_disabled_is_reported() {
        let s = sandbox();
        let path = s.places.target(Harness::Codex, Scope::User);
        let ours = hook_command(&s.binary, Harness::Codex);
        std::fs::write(
            &path,
            format!("[[hooks.UserPromptSubmit]]\n[[hooks.UserPromptSubmit.hooks]]\ncommand = \"{ours}\"\ntype = \"command\"\n"),
        )
        .unwrap();
        assert_eq!(
            audit(&s.places, &s.binary, Harness::Codex, Scope::User).state,
            "stale"
        );
        let _ = install(&s, &[Harness::Codex]);
        let text = std::fs::read_to_string(&path).unwrap();
        std::fs::write(&path, format!("[features]\nhooks = false\n\n{text}")).unwrap();
        assert_eq!(
            audit(&s.places, &s.binary, Harness::Codex, Scope::User).state,
            "disabled"
        );
    }

    #[test]
    fn extensions_are_written_refreshed_and_never_clobber_foreign_files() {
        let s = sandbox();
        let changes = install(&s, &[Harness::Copilot, Harness::Pi, Harness::Omp]);
        assert_eq!(actions(&changes), ["created", "created", "created"]);
        for h in [Harness::Copilot, Harness::Pi, Harness::Omp] {
            let text = std::fs::read_to_string(s.places.target(h, Scope::User)).unwrap();
            assert!(
                text.contains(MARKER) && text.contains(&format!("\"--harness\", \"{}\"", h.slug()))
            );
            assert!(text.contains(&serde_json::to_string(&s.binary.to_string_lossy()).unwrap()));
            assert_eq!(
                audit(&s.places, &s.binary, h, Scope::User).state,
                "installed"
            );
        }
        let moved = PathBuf::from("/opt/flowspace3");
        assert_eq!(
            audit(&s.places, &moved, Harness::Pi, Scope::User).state,
            "stale"
        );

        let pi = s.places.target(Harness::Pi, Scope::User);
        std::fs::write(&pi, "export default function () {}").unwrap();
        assert_eq!(
            audit(&s.places, &s.binary, Harness::Pi, Scope::User).state,
            "conflict"
        );
        assert_eq!(actions(&install(&s, &[Harness::Pi])), ["failed"]);
        assert_eq!(
            std::fs::read_to_string(&pi).unwrap(),
            "export default function () {}"
        );

        let removed = apply(
            &s.places,
            &s.binary,
            &[Harness::Copilot],
            Scope::User,
            false,
            false,
            2,
        );
        assert_eq!(actions(&removed), ["removed"]);
        assert_eq!(removed[0].backup, None);
        assert!(
            !s.places
                .target(Harness::Copilot, Scope::User)
                .parent()
                .unwrap()
                .exists()
        );
    }

    #[test]
    fn all_skips_harnesses_the_user_does_not_have() {
        let s = sandbox();
        std::fs::remove_dir_all(s.places.root(Harness::Omp)).unwrap();
        let changes = apply(
            &s.places,
            &s.binary,
            &Harness::ALL,
            Scope::User,
            true,
            true,
            1,
        );
        assert_eq!(
            actions(&changes),
            ["created", "created", "created", "created", "skipped"]
        );
        assert!(!s.places.target(Harness::Omp, Scope::User).exists());
    }

    #[test]
    fn project_scope_writes_inside_the_repo() {
        let s = sandbox();
        let _ = apply(
            &s.places,
            &s.binary,
            &[Harness::Claude, Harness::Pi],
            Scope::Project,
            false,
            true,
            1,
        );
        assert!(
            s.places
                .project
                .join(".claude/settings.local.json")
                .is_file()
        );
        assert!(
            s.places
                .project
                .join(".pi/extensions/flowspace3.ts")
                .is_file()
        );
        assert!(!s.places.target(Harness::Claude, Scope::User).exists());
    }

    #[test]
    fn unreadable_configs_fail_without_writing() {
        let s = sandbox();
        let path = s.places.target(Harness::Claude, Scope::User);
        std::fs::write(&path, "{ not json").unwrap();
        let changes = install(&s, &[Harness::Claude]);
        assert_eq!(actions(&changes), ["failed"]);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{ not json");
    }

    #[test]
    fn commands_quote_awkward_paths() {
        assert_eq!(
            hook_command(Path::new("/Users/x/My Tools/flowspace3"), Harness::Claude),
            "'/Users/x/My Tools/flowspace3' hooks prompt --harness claude"
        );
    }
}
