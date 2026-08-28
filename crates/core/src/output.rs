//! Who is reading stdout, and therefore what shape the answer takes.
//!
//! # The ruling
//!
//! Jordan, 2026-08-28, verbatim: *"let's default to human output, with --json
//! available"*.
//!
//! Read literally that would break every agent in the field, so it is
//! implemented the way the human-render prototype asserted and Jordan accepted:
//! a TTY gets the human rendering, and anything that is NOT a terminal — a
//! pipe, a file, a CI log, an agent's captured subprocess — keeps getting the
//! frozen JSON envelope with no flag at all. `flowspace3 search … | jq` must
//! keep working untouched; that property is v1's and it is not being spent.
//!
//! # The rule
//!
//! ```text
//! --json given?      ──yes──▶ Json
//!      │no
//! --human given?     ──yes──▶ Human
//!      │no
//! FS3_OUTPUT set?    ──json/human──▶ that
//!      │no (or unrecognised/auto)
//! stdout a terminal? ──no───▶ Json
//!      ▼yes
//!    Human
//! ```
//!
//! Flags beat the environment because they are the more recent decision, and
//! the environment beats the terminal probe because a harness that exports
//! [`OUTPUT_ENV`] is stating what it wants in a place a probe cannot see. That
//! last step is the one that matters for this project's own fleet: agents run
//! inside tmux PTYs, so the terminal probe says "human" and is WRONG. Such a
//! harness exports `FS3_OUTPUT=json` once and stops thinking about it.
//!
//! # Why this is a function in core, not an `if` in the CLI
//!
//! It is the one place the question is answered, it is pure, and it is
//! exhaustively tested below. A second copy of this decision anywhere — the
//! TUI, a future web shim, the daemon rendering into a log — would be a second
//! answer to a question that must only have one.

/// What stdout should receive.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OutputMode {
    /// The frozen envelope, verbatim. The contract every agent reads.
    #[default]
    Json,
    /// The rendered human screen, for a person at a terminal.
    Human,
}

impl OutputMode {
    /// The wire/env spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            OutputMode::Json => "json",
            OutputMode::Human => "human",
        }
    }
}

/// The environment override, for harnesses that must not be guessed about.
pub const OUTPUT_ENV: &str = "FS3_OUTPUT";

/// The value that explicitly asks for the terminal probe.
///
/// Spelled out so a harness can neutralise an inherited `FS3_OUTPUT` without
/// having to unset a variable it may not own.
pub const OUTPUT_AUTO: &str = "auto";

/// Resolve the output mode.
///
/// `env` is the raw value of [`OUTPUT_ENV`], if any; case and surrounding
/// whitespace are forgiven, and anything unrecognised is IGNORED rather than
/// rejected — an unreadable environment variable must not turn every command
/// into a usage error, and the fall-through is the safe direction.
///
/// When both flags are given, `--json` wins. The two answers are not equally
/// costly: a person handed JSON is inconvenienced, an agent handed prose is
/// broken. (The CLI also marks the flags as conflicting, so this is a backstop
/// for a caller that reaches the function directly.)
#[must_use]
pub fn resolve(
    stdout_is_terminal: bool,
    json_flag: bool,
    human_flag: bool,
    env: Option<&str>,
) -> OutputMode {
    if json_flag {
        return OutputMode::Json;
    }
    if human_flag {
        return OutputMode::Human;
    }

    match env.map(str::trim) {
        Some(value) if value.eq_ignore_ascii_case(OutputMode::Json.as_str()) => OutputMode::Json,
        Some(value) if value.eq_ignore_ascii_case(OutputMode::Human.as_str()) => OutputMode::Human,
        _ if stdout_is_terminal => OutputMode::Human,
        _ => OutputMode::Json,
    }
}

#[cfg(test)]
mod tests {
    use super::{OutputMode, resolve};

    /// Every input combination, stated as a table rather than as prose.
    ///
    /// The columns are the four inputs in precedence order; a reader checking
    /// whether the rule is right never has to reconstruct it from branches.
    #[test]
    fn the_precedence_table_is_exhaustive() {
        // (tty, --json, --human, env) -> mode
        let cases: &[(bool, bool, bool, Option<&str>, OutputMode)] = &[
            // --json wins from anywhere, including over --human.
            (true, true, false, None, OutputMode::Json),
            (false, true, false, None, OutputMode::Json),
            (true, true, true, Some("human"), OutputMode::Json),
            (false, true, false, Some("human"), OutputMode::Json),
            // --human forces the renderer, pipe or not.
            (true, false, true, None, OutputMode::Human),
            (false, false, true, None, OutputMode::Human),
            (false, false, true, Some("json"), OutputMode::Human),
            // The environment decides when no flag does.
            (true, false, false, Some("json"), OutputMode::Json),
            (false, false, false, Some("human"), OutputMode::Human),
            (true, false, false, Some("JSON"), OutputMode::Json),
            (false, false, false, Some("  human  "), OutputMode::Human),
            // Unrecognised, empty and `auto` fall through to the probe.
            (true, false, false, Some("yaml"), OutputMode::Human),
            (false, false, false, Some("yaml"), OutputMode::Json),
            (true, false, false, Some(""), OutputMode::Human),
            (true, false, false, Some("auto"), OutputMode::Human),
            (false, false, false, Some("auto"), OutputMode::Json),
            // The probe alone.
            (true, false, false, None, OutputMode::Human),
            (false, false, false, None, OutputMode::Json),
        ];

        for &(tty, json, human, env, expected) in cases {
            assert_eq!(
                resolve(tty, json, human, env),
                expected,
                "tty={tty} --json={json} --human={human} env={env:?}"
            );
        }
    }

    /// The property the whole plan rests on, stated on its own so it cannot be
    /// lost in a table edit: with no flag and no environment, a thing that is
    /// not a terminal gets JSON.
    #[test]
    fn a_pipe_with_no_flags_still_gets_json() {
        assert_eq!(resolve(false, false, false, None), OutputMode::Json);
    }

    /// The tmux-PTY case: an agent that looks like a terminal, and says so.
    #[test]
    fn a_harness_can_override_a_terminal_that_is_really_an_agent() {
        assert_eq!(resolve(true, false, false, Some("json")), OutputMode::Json);
    }
}
