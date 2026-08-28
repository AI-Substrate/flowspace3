//! The bundled agent skill and its install command (PRD req-0053).
//!
//! The skill that teaches an agent to USE flowspace ships INSIDE this binary, so
//! a distribution copy exists wherever the binary does. Getting that copy into
//! an agent's home directories is an EXPLICIT command (`flowspace3 doctor
//! install-skill`): nothing writes these files silently or by force, and
//! `doctor` itself never installs — it only reports.
//!
//! # Why the copy lives in this crate
//!
//! Same ruling as the bundled docs (docs.rs): the bundle has to be part of the
//! package, not near it. The repository's `.agents/skills/flowspace/SKILL.md`
//! is a relative symlink into this file — one copy, so the in-repo text and the
//! distributed text cannot drift.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context;
use fs3_core::envelope::Envelope;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The bundled skill, compiled in.
const SKILL: &str = include_str!("../skills/flowspace/SKILL.md");

/// The skill's directory name, as installed beneath a skills root.
const SKILL_DIR: &str = "flowspace";

/// The skill file's name within its directory.
const SKILL_FILE: &str = "SKILL.md";

/// The skills roots the install command writes into, relative to `$HOME`.
const SKILL_ROOTS: [&str; 2] = [".agents/skills", ".claude/skills"];

/// What one skills root reports after a sync.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetOutcome {
    /// Where the skill now lives.
    pub path: PathBuf,
    /// What happened to it.
    pub state: TargetState,
}

/// The state of one target after a sync.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetState {
    /// Was absent; the bundled content was written.
    Installed,
    /// Was present under another hash; overwritten with the bundled content.
    Updated,
    /// Was present and current; left untouched.
    Current,
}

/// What `doctor install-skill` answers with.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallReport {
    /// The content hash of the bundled skill.
    pub hash: String,
    /// One outcome per skills root, in fixed order.
    pub targets: Vec<TargetOutcome>,
}

/// The state of one skills root, as read without writing (req-0053's doctor row).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RootState {
    /// Present and matching the bundled hash.
    Current,
    /// Present under another hash.
    Stale,
    /// Absent.
    Missing,
}

/// Read-only audit of the skills roots beneath a home directory.
///
/// Pure: explicit paths, no env, no writes — doctor's row reports from this
/// without ever installing, and tests run it against temp dirs. Fixed order:
/// one state per root, in `SKILL_ROOTS` order.
pub fn audit(home: &Path) -> Vec<RootState> {
    let hash = sha256_hex(SKILL.as_bytes());
    SKILL_ROOTS
        .map(|root| audit_root(&home.join(root), &hash))
        .to_vec()
}

fn audit_root(root: &Path, bundled_hash: &str) -> RootState {
    match fs::read(root.join(SKILL_DIR).join(SKILL_FILE)) {
        Ok(existing) if sha256_hex(&existing) == bundled_hash => RootState::Current,
        Ok(_) => RootState::Stale,
        Err(_) => RootState::Missing,
    }
}

/// Install or update the bundled skill into every skills root.
///
/// Fails (anyhow, exit-1) when `$HOME` cannot be located or a write fails — the
/// same bootstrap-grade handling as `settings::config_dir`, not a cataloged
/// error: POSIX essentially cannot produce the former, and the latter names the
/// exact path that failed.
pub fn install() -> anyhow::Result<Envelope<InstallReport>> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .context("cannot locate the skills roots: HOME is not set")?;

    let hash = sha256_hex(SKILL.as_bytes());
    let mut targets = Vec::with_capacity(SKILL_ROOTS.len());
    for root in SKILL_ROOTS {
        let outcome = sync_root(&home.join(root), SKILL, &hash)
            .with_context(|| format!("syncing the skills root under {home:?}/{root}"))?;
        targets.push(outcome);
    }

    Ok(Envelope::ok("doctor", InstallReport { hash, targets }))
}

/// Sync one skills root against the bundled content.
///
/// Pure: takes explicit paths, so tests never touch a real `$HOME`. Overwriting
/// a stale copy is what "updates" means in req-0053 — the bundled text is the
/// only truth this command writes, and every overwrite is reported, never
/// silent.
fn sync_root(root: &Path, bundled: &str, bundled_hash: &str) -> std::io::Result<TargetOutcome> {
    let path = root.join(SKILL_DIR).join(SKILL_FILE);
    let state = match fs::read(&path) {
        Ok(existing) if sha256_hex(&existing) == bundled_hash => TargetState::Current,
        Ok(_) => {
            fs::write(&path, bundled)?;
            TargetState::Updated
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&path, bundled)?;
            TargetState::Installed
        }
        Err(error) => return Err(error),
    };
    Ok(TargetOutcome { path, state })
}

/// The sha256 of some bytes, hex.
fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A throwaway skills root, unique per test and per process.
    fn temp_root(label: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("fs3-skill-{label}-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn sha256_hex_known_vector() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn bundled_content_is_the_skill() {
        assert!(SKILL.starts_with("---\nname: flowspace\n"));
        assert!(SKILL.contains("flowspace3 search"));
    }

    #[test]
    fn the_front_door_introduces_every_query_verb() {
        // `ask` merged with this skill not mentioning it, so agents kept
        // reaching for `search` on question-shaped work — while the search
        // envelope's own hint pointed them at a verb the front door never
        // introduced. A verb the skill does not teach may as well not exist to
        // the agents this file is written for, so the set is asserted rather
        // than left to whoever remembers.
        for verb in ["flowspace3 search", "flowspace3 get", "flowspace3 ask"] {
            assert!(SKILL.contains(verb), "the skill must introduce `{verb}`");
        }
        // Scope discipline is the half that keeps `ask` affordable on a
        // many-repo index: widening is a deliberate act, not a default.
        assert!(SKILL.contains("--repo all"));
    }

    #[test]
    fn sync_root_lifecycle() {
        let root = temp_root("lifecycle");
        let hash = sha256_hex(SKILL.as_bytes());

        // Absent -> written, reported Installed.
        let first = sync_root(&root, SKILL, &hash).unwrap();
        assert_eq!(first.state, TargetState::Installed);
        assert_eq!(fs::read_to_string(&first.path).unwrap(), SKILL);

        // Present and current -> untouched.
        let second = sync_root(&root, SKILL, &hash).unwrap();
        assert_eq!(second.state, TargetState::Current);

        // Present and stale -> overwritten with the bundled text.
        fs::write(&first.path, "a stale copy").unwrap();
        let third = sync_root(&root, SKILL, &hash).unwrap();
        assert_eq!(third.state, TargetState::Updated);
        assert_eq!(fs::read_to_string(&third.path).unwrap(), SKILL);

        // Idempotent again.
        let fourth = sync_root(&root, SKILL, &hash).unwrap();
        assert_eq!(fourth.state, TargetState::Current);

        fs::remove_dir_all(&root).unwrap();
    }

    /// req-0053: the audit reads what install would write — and writes nothing.
    #[test]
    fn audit_reports_the_roots_without_touching_them() {
        let home = temp_root("audit");

        assert_eq!(
            audit(&home),
            vec![RootState::Missing, RootState::Missing],
            "an untouched home has no installed copies"
        );

        for root in SKILL_ROOTS {
            let dir = home.join(root).join(SKILL_DIR);
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join(SKILL_FILE), SKILL).unwrap();
        }
        assert_eq!(
            audit(&home),
            vec![RootState::Current, RootState::Current],
            "the bundled bytes are current by definition"
        );

        fs::write(
            home.join(SKILL_ROOTS[0]).join(SKILL_DIR).join(SKILL_FILE),
            "not the bundled bytes",
        )
        .unwrap();
        assert_eq!(audit(&home), vec![RootState::Stale, RootState::Current]);

        fs::remove_dir_all(&home).unwrap();
    }
}
