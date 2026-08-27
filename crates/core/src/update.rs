//! The auto-updater's domain: what state the installation is in, what that
//! state should be telling the user, and which of two versions is newer
//! (PRD req 54).
//!
//! Pure, and deliberately so. Probing GitHub, downloading an asset and
//! renaming a file are effects and live in the daemon; deciding whether a
//! version is worth installing and what to say about it is a decision, and
//! decisions are testable without a network or a database (workshop 001
//! rule 2).

use crate::messages::{Severity, UserMessage};

/// The source every message the updater raises is filed under.
///
/// One producer owns one source: [`UpdateState::desired_messages`] declares
/// the whole of it on every pass, and the queue retracts anything under this
/// name that the declaration left out.
pub const UPDATE_SOURCE: &str = "update";

/// The installer of last resort, named in the message a blocked update raises.
pub const REINSTALL_COMMAND: &str =
    "curl -fsSL https://raw.githubusercontent.com/AI-Substrate/flowspace3/main/install.sh | sh";

/// What the updater knows right now, as the store holds it, for ONE
/// installation.
///
/// Keyed by [`UpdateState::install_path`], because a store is shared and an
/// **install is a path**. One row per store was wrong in a way that only
/// showed up on a machine with two installs: `install.sh` picks
/// `/usr/local/bin` or `~/.local/bin` depending on permissions, so anyone who
/// has ever installed both ways has two installs against one database, and a
/// single row thrashed last-writer-wins between them. Root ended up carrying
/// another user's "not writable" message about a path root does not use — on
/// a surface whose `next_action` is NOT NULL precisely to guarantee it is
/// actionable (Jordan ruled per-install truth, 2026-08-27).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UpdateState {
    /// WHICH installation this is the state of: a daemon's own resolved
    /// binary path, canonicalised. The row's identity rather than a payload
    /// field, which is the whole of the per-install fix.
    pub install_path: String,
    /// The newest version the probe has seen, installed or not.
    pub latest_seen: Option<String>,
    /// What the binary at [`UpdateState::install_path`] said it was when it
    /// was last asked — a **cache of disk**, not a memory of what the updater
    /// did.
    ///
    /// That distinction is the fix for a real defect. The old field recorded
    /// "we swapped 0.3.1 in" and nothing could ever unset it, so a pinned
    /// reinstall at an older tag left a permanently false "restart to pick up
    /// 0.3.1" against a path holding 0.3.0. Re-read from the file on every
    /// check, a swap and an out-of-band change produce the same answer,
    /// because both are read from the same place. `None` means the path holds
    /// nothing that can be asked what it is.
    pub installed_version: Option<String>,
    /// Why the last attempt did not install, when it did not.
    pub blocked_reason: Option<String>,
    /// When a daemon last asked GitHub, RFC 3339 UTC. `None` means never.
    pub last_checked: Option<String>,
}

impl UpdateState {
    /// The messages this state should be raising right now, given the version
    /// of the process asking.
    ///
    /// This function *is* the clear-condition story (PRD req 59). There is no
    /// stored predicate anywhere: each pass declares the messages that are
    /// true now, and the queue retracts the rest. An update that succeeds
    /// stops declaring "restart me" the moment the running version matches,
    /// so the message disappears without anything having evaluated a rule.
    ///
    /// # Every key names the install it is about
    ///
    /// `user_messages.key` is the PRIMARY KEY, so two installs sharing a
    /// store could not otherwise both hold `update:blocked` — one would
    /// silently overwrite the other, which is the same last-writer-wins
    /// defect one level up. The path in the key is what keeps them distinct
    /// rows. Scoping *ownership* and *visibility* is a different job, done by
    /// the queue's own `install_path` column: string-matching a key is not a
    /// mechanism.
    #[must_use]
    pub fn desired_messages(&self, running_version: &str) -> Vec<UserMessage> {
        let mut messages = Vec::new();
        let path = self.install_path.as_str();

        // A newer binary is on disk that this process is not running. Warning
        // rather than info: the user turned auto-update on and is not yet
        // getting the version they were promised.
        if let Some(installed) = &self.installed_version
            && installed != running_version
        {
            messages.push(UserMessage::new(
                format!("update:installed:{installed}:{path}"),
                UPDATE_SOURCE,
                Severity::Warning,
                format!(
                    "flowspace3 {installed} is installed at {path}; this daemon is still \
                     running {running_version}"
                ),
                "restart the fs3 daemon to pick it up",
            ));
        }

        // Something newer exists and could not be installed. This is the
        // message that has to earn its keep on a root-owned install path, so it
        // names the path, the reason, and both ways out (Jordan, 2026-08-27).
        if let Some(reason) = &self.blocked_reason {
            let newer = self
                .latest_seen
                .as_deref()
                .map_or(String::new(), |latest| format!(" ({latest} is available)"));
            messages.push(UserMessage::new(
                format!("update:blocked:{path}"),
                UPDATE_SOURCE,
                Severity::Warning,
                format!("update not possible at {path}: {reason}{newer}"),
                format!(
                    "run `flowspace3 doctor upgrade` from a shell that can write that path \
                     — or reinstall: `{REINSTALL_COMMAND}`"
                ),
            ));
        }

        messages
    }
}

/// A release version, compared the way semver compares.
///
/// Deliberately not a semver crate: fs3 publishes `vX.Y.Z` tags from
/// release-please and nothing else, so the whole of the grammar in play is
/// three numbers. A dependency here would buy pre-release ordering rules that
/// no fs3 tag can exercise.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    /// Breaking.
    pub major: u64,
    /// Additive.
    pub minor: u64,
    /// Fixes.
    pub patch: u64,
}

impl Version {
    /// Parse `1.2.3` or `v1.2.3`.
    ///
    /// Anything else is `None` rather than a lenient guess: a probe that
    /// cannot read the version it found must not decide it is newer than what
    /// is running.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let text = text.strip_prefix('v').unwrap_or(text);
        // A tag carrying build metadata or a pre-release is not something
        // release-please produces here, and guessing at its ordering is how an
        // updater installs a release candidate over a stable build.
        if text.contains(['-', '+']) {
            return None;
        }

        let mut parts = text.split('.');
        let mut next = || parts.next()?.parse::<u64>().ok();
        let version = Self {
            major: next()?,
            minor: next()?,
            patch: next()?,
        };
        parts.next().is_none().then_some(version)
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// Is `candidate` worth installing over `running`?
///
/// Strictly newer, both parseable. A same-version answer is a no-op and a
/// lower one is a downgrade — the updater performs neither, so a release that
/// is yanked and replaced by a lower tag cannot walk an installation backwards.
#[must_use]
pub fn is_upgrade(running: &str, candidate: &str) -> bool {
    match (Version::parse(running), Version::parse(candidate)) {
        (Some(running), Some(candidate)) => candidate > running,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_clean_state_says_nothing() {
        assert!(UpdateState::default().desired_messages("0.2.0").is_empty());
    }

    #[test]
    fn an_installed_binary_the_daemon_is_not_running_asks_for_a_restart() {
        let state = UpdateState {
            installed_version: Some("0.3.0".into()),
            install_path: "/usr/local/bin/flowspace3".into(),
            ..UpdateState::default()
        };

        let messages = state.desired_messages("0.2.0");
        assert_eq!(messages.len(), 1);
        assert_eq!(
            messages[0].key,
            "update:installed:0.3.0:/usr/local/bin/flowspace3"
        );
        assert_eq!(
            messages[0].next_action,
            "restart the fs3 daemon to pick it up"
        );
        assert!(messages[0].text.contains("/usr/local/bin/flowspace3"));
    }

    #[test]
    fn the_restart_message_clears_itself_once_the_daemon_is_that_version() {
        let state = UpdateState {
            installed_version: Some("0.3.0".into()),
            ..UpdateState::default()
        };

        assert!(state.desired_messages("0.3.0").is_empty());
    }

    /// The per-install fix, in the domain: the same situation at two paths is
    /// two messages, not one row two installs fight over. `key` is the queue's
    /// PRIMARY KEY, so a shared key here would mean root's daemon silently
    /// overwriting a message about a path root does not use.
    #[test]
    fn the_same_condition_at_two_install_paths_produces_two_distinct_keys() {
        let at = |path: &str| UpdateState {
            latest_seen: Some("0.3.0".into()),
            installed_version: Some("0.3.0".into()),
            install_path: path.into(),
            blocked_reason: Some("not writable".into()),
            ..UpdateState::default()
        };

        let root: Vec<String> = at("/usr/local/bin/flowspace3")
            .desired_messages("0.2.0")
            .into_iter()
            .map(|message| message.key)
            .collect();
        let alice: Vec<String> = at("/home/alice/.local/bin/flowspace3")
            .desired_messages("0.2.0")
            .into_iter()
            .map(|message| message.key)
            .collect();

        assert_eq!(root.len(), 2);
        assert_eq!(alice.len(), 2);
        for key in &root {
            assert!(
                !alice.contains(key),
                "{key} is claimed by both installs — one would overwrite the other"
            );
        }
    }

    #[test]
    fn a_blocked_update_names_the_path_the_reason_and_both_ways_out() {
        let state = UpdateState {
            latest_seen: Some("0.3.0".into()),
            install_path: "/usr/local/bin/flowspace3".into(),
            blocked_reason: Some("/usr/local/bin is not writable by this user".into()),
            ..UpdateState::default()
        };

        let messages = state.desired_messages("0.2.0");
        assert_eq!(messages.len(), 1);
        let message = &messages[0];
        assert_eq!(message.key, "update:blocked:/usr/local/bin/flowspace3");
        assert!(message.text.contains("/usr/local/bin/flowspace3"));
        assert!(message.text.contains("not writable"));
        assert!(message.text.contains("0.3.0 is available"));
        assert!(message.next_action.contains("doctor upgrade"));
        assert!(message.next_action.contains("install.sh"));
    }

    #[test]
    fn a_landed_swap_and_a_later_block_both_speak() {
        let state = UpdateState {
            latest_seen: Some("0.4.0".into()),
            installed_version: Some("0.3.0".into()),
            install_path: "/usr/local/bin/flowspace3".into(),
            blocked_reason: Some("checksum mismatch".into()),
            ..UpdateState::default()
        };

        let keys: Vec<String> = state
            .desired_messages("0.2.0")
            .into_iter()
            .map(|message| message.key)
            .collect();
        assert_eq!(
            keys,
            [
                "update:installed:0.3.0:/usr/local/bin/flowspace3",
                "update:blocked:/usr/local/bin/flowspace3"
            ]
        );
    }

    #[test]
    fn versions_parse_with_or_without_the_tag_prefix() {
        assert_eq!(
            Version::parse("v1.2.3"),
            Some(Version {
                major: 1,
                minor: 2,
                patch: 3
            })
        );
        assert_eq!(Version::parse("1.2.3"), Version::parse("v1.2.3"));
    }

    #[test]
    fn a_version_that_is_not_three_numbers_is_refused_rather_than_guessed() {
        for refused in ["1.2", "1.2.3.4", "1.2.x", "v1.2.3-rc.1", "1.2.3+build", ""] {
            assert_eq!(Version::parse(refused), None, "{refused} should not parse");
        }
    }

    #[test]
    fn ordering_is_numeric_not_lexical() {
        // The bug this test exists for: "0.10.0" sorts BEFORE "0.9.0" as text.
        assert!(is_upgrade("0.9.0", "0.10.0"));
        assert!(!is_upgrade("0.10.0", "0.9.0"));
    }

    #[test]
    fn the_updater_never_reinstalls_or_downgrades() {
        assert!(!is_upgrade("0.2.0", "0.2.0"), "same version is a no-op");
        assert!(
            !is_upgrade("0.3.0", "0.2.9"),
            "lower version is a downgrade"
        );
        assert!(is_upgrade("0.2.0", "0.2.1"));
        assert!(is_upgrade("0.2.0", "1.0.0"));
    }

    #[test]
    fn an_unparseable_version_on_either_side_is_never_an_upgrade() {
        assert!(!is_upgrade("nightly", "0.3.0"));
        assert!(!is_upgrade("0.2.0", "latest"));
    }
}
