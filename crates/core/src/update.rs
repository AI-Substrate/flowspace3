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

/// What the updater knows right now, as the store holds it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UpdateState {
    /// The newest version the probe has seen, installed or not.
    pub latest_seen: Option<String>,
    /// The version sitting at [`UpdateState::install_path`] because the daemon
    /// put it there. `Some`, plus a running process of a different version, is
    /// exactly the restart-me condition.
    pub installed_version: Option<String>,
    /// Where the swap happened, canonicalised.
    pub install_path: Option<String>,
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
    #[must_use]
    pub fn desired_messages(&self, running_version: &str) -> Vec<UserMessage> {
        let mut messages = Vec::new();
        let path = self.install_path.as_deref().unwrap_or("the install path");

        // A newer binary is on disk that this process is not running. Warning
        // rather than info: the user turned auto-update on and is not yet
        // getting the version they were promised.
        if let Some(installed) = &self.installed_version
            && installed != running_version
        {
            messages.push(UserMessage::new(
                format!("update:installed:{installed}"),
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
                "update:blocked",
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
            install_path: Some("/usr/local/bin/flowspace3".into()),
            ..UpdateState::default()
        };

        let messages = state.desired_messages("0.2.0");
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].key, "update:installed:0.3.0");
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

    #[test]
    fn a_blocked_update_names_the_path_the_reason_and_both_ways_out() {
        let state = UpdateState {
            latest_seen: Some("0.3.0".into()),
            install_path: Some("/usr/local/bin/flowspace3".into()),
            blocked_reason: Some("/usr/local/bin is not writable by this user".into()),
            ..UpdateState::default()
        };

        let messages = state.desired_messages("0.2.0");
        assert_eq!(messages.len(), 1);
        let message = &messages[0];
        assert_eq!(message.key, "update:blocked");
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
            install_path: Some("/usr/local/bin/flowspace3".into()),
            blocked_reason: Some("checksum mismatch".into()),
            ..UpdateState::default()
        };

        let keys: Vec<String> = state
            .desired_messages("0.2.0")
            .into_iter()
            .map(|message| message.key)
            .collect();
        assert_eq!(keys, ["update:installed:0.3.0", "update:blocked"]);
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
