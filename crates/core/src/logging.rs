//! Where the daemon's log file lives, and what to say when it cannot be
//! written (PRD req 59 for the message; the 2026-08-27 summarize-lane incident
//! for the rest).
//!
//! Pure, like everything else in core: [`resolve_log_dir`] takes the home
//! directory as an argument rather than reading the environment, so both the
//! daemon (which writes the file) and `flowspace3 doctor` (which reports where
//! it is) answer from ONE implementation and cannot drift apart.
//!
//! # Why a tilde rather than a resolved default
//!
//! `daemon.log_dir` defaults to the literal string `~/.local/state/flowspace3/logs`.
//! A default that resolved `$HOME` eagerly would make [`crate::Config::default`]
//! machine-dependent — and core is the crate that performs no effects, so it is
//! the one place that must not read an environment variable. The tilde keeps
//! the default printable (`flowspace3 config show` shows the same string on
//! every machine), pure, and honest about what it means.

use std::path::{Path, PathBuf};

use crate::messages::{Severity, UserMessage};

/// The source every message about logging is filed under.
///
/// One producer owns one source, exactly as [`crate::SCHEMA_SOURCE`] and
/// [`crate::UPDATE_SOURCE`] do: the daemon declares the whole of this source
/// once at startup, so a run that CAN write its log retracts the previous
/// run's complaint without any clearing logic of its own.
pub const LOGGING_SOURCE: &str = "logging";

/// The active log file's name inside [`crate::DaemonConfig::log_dir`].
///
/// Rolled files are this name plus `.1`, `.2`, … — oldest highest. Numbered
/// rather than timestamped so the names are deterministic: `doctor` can name
/// the active file, a test can assert on file counts, and two rolls inside one
/// second cannot collide.
pub const LOG_FILE_NAME: &str = "flowspace3.log";

/// The rolled file `generation` back from the active one.
///
/// Generation 0 is the active file itself.
#[must_use]
pub fn rolled_name(generation: u32) -> String {
    if generation == 0 {
        LOG_FILE_NAME.to_string()
    } else {
        format!("{LOG_FILE_NAME}.{generation}")
    }
}

/// Turn a configured `log_dir` into a real path.
///
/// A leading `~` (alone, or followed by a separator) is the user's home;
/// anything else is taken as given, absolute or relative to the working
/// directory. `home` is `None` when the caller could not determine one, which
/// is only a failure for a path that actually needs it.
///
/// # Errors
/// A string that needs a home directory when the caller has none. The message
/// is written to be shown to a user as-is.
pub fn resolve_log_dir(configured: &str, home: Option<&Path>) -> Result<PathBuf, String> {
    let trimmed = configured.trim();
    if trimmed.is_empty() {
        return Err("daemon.log_dir is empty".to_string());
    }

    let Some(rest) = trimmed.strip_prefix('~') else {
        return Ok(PathBuf::from(trimmed));
    };

    // `~user` is deliberately not supported: expanding another user's home
    // needs a password database, which is an effect, and no fs3 path has ever
    // meant it. Treating it as a literal directory name is the honest reading.
    let rest = rest.trim_start_matches(['/', '\\']);
    let home = home.ok_or_else(|| {
        format!("{configured} starts with ~ but HOME is not set, so there is no home to expand")
    })?;

    Ok(if rest.is_empty() {
        home.to_path_buf()
    } else {
        home.join(rest)
    })
}

/// What the queue should say when the daemon could not open its log file.
///
/// Warning rather than error: the daemon is running and serving, and its log
/// is still on stdout — what has been lost is the DURABLE copy, which is
/// exactly the thing nobody notices missing until an incident (the summarize
/// lane died on 2026-08-27 and its panic existed only in a terminal).
///
/// The key carries the directory, so pointing `log_dir` somewhere else raises
/// a genuinely new message rather than reusing a key the user has dismissed.
#[must_use]
pub fn unwritable_message(directory: &str, reason: &str) -> UserMessage {
    UserMessage::new(
        format!("logging:unwritable:{directory}"),
        LOGGING_SOURCE,
        Severity::Warning,
        format!(
            "the daemon cannot write its log file in {directory} ({reason}); it is logging to \
             stdout only, so nothing survives this process"
        ),
        "point `[daemon] log_dir` at a writable directory (or fix its permissions) and restart \
         the daemon — `flowspace3 doctor` names the path it would use",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home() -> PathBuf {
        PathBuf::from("/home/agent")
    }

    #[test]
    fn a_tilde_path_expands_against_the_home_it_is_given() {
        let resolved = resolve_log_dir("~/.local/state/flowspace3/logs", Some(&home()))
            .expect("a home was supplied");
        assert_eq!(
            resolved,
            PathBuf::from("/home/agent/.local/state/flowspace3/logs")
        );
    }

    #[test]
    fn a_bare_tilde_is_the_home_itself() {
        assert_eq!(resolve_log_dir("~", Some(&home())).unwrap(), home());
    }

    #[test]
    fn an_absolute_path_is_left_exactly_as_written() {
        // Including on a machine with no home: nothing about /var/log needs one.
        assert_eq!(
            resolve_log_dir("/var/log/flowspace3", None).unwrap(),
            PathBuf::from("/var/log/flowspace3")
        );
    }

    /// The failure has to name the variable, because "cannot expand ~" tells a
    /// reader nothing they can act on.
    #[test]
    fn a_tilde_path_without_a_home_fails_naming_home() {
        let error = resolve_log_dir("~/logs", None).expect_err("no home was supplied");
        assert!(error.contains("HOME"), "{error}");
    }

    #[test]
    fn rolled_names_count_back_from_the_active_file() {
        assert_eq!(rolled_name(0), "flowspace3.log");
        assert_eq!(rolled_name(1), "flowspace3.log.1");
        assert_eq!(rolled_name(4), "flowspace3.log.4");
    }

    /// Two directories are two conditions: a user who moves `log_dir` after
    /// dismissing the first message must hear about the second.
    #[test]
    fn the_message_key_carries_the_directory() {
        let first = unwritable_message("/var/log/fs3", "permission denied");
        let second = unwritable_message("/srv/fs3", "permission denied");
        assert_ne!(first.key, second.key);
        assert_eq!(first.source, LOGGING_SOURCE);
        assert_eq!(first.severity, Severity::Warning);
    }
}
