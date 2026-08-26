//! The TTY strategy: who is reading, and therefore what to print.
//!
//! One decision, made once, at the boundary — never inside a surface.
//!
//! ```text
//! --json given?  ────yes──▶ JSON (the envelope, verbatim)
//!      │no
//! stdout a tty?  ────no───▶ JSON (something is consuming this — a pipe, a
//!      │yes                       file, a CI log, an agent)
//!      ▼
//!    RICH
//! ```
//!
//! # Why "piped ⇒ JSON", not "piped ⇒ rich-without-colour"
//!
//! Because the two skins carry the same truth, degrading to plain text would
//! throw information away for no one's benefit: a program on the other end of
//! the pipe wants the envelope, and a human redirecting to a file wants
//! something they can grep or replay. `flowspace3 search … | jq` therefore
//! works with no flag at all, which is the property v1's JSON-only surface has
//! today and must not lose when the human skin lands.
//!
//! A human who genuinely wants the rich rendering in a file can ask
//! (`--human`); the point is that the DEFAULT never surprises a script.
//!
//! # Colour is a third, independent question
//!
//! Rich-vs-JSON is about the SHAPE of the answer; colour is about the
//! capabilities of the terminal. [`anstream`] answers the second one and
//! already knows the whole matrix — `NO_COLOR`, `CLICOLOR`, `CLICOLOR_FORCE`,
//! `TERM=dumb`, Windows console modes. The renderer emits ANSI always and this
//! module hands anstream the user's override; nothing else in the crate has an
//! opinion.

use std::io::{IsTerminal, Stdout};

use anstream::{AutoStream, ColorChoice};

/// What shape the answer takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Tables, colour, glyphs — for a person at a terminal.
    Rich,
    /// The envelope, verbatim, for whatever is on the other end of the pipe.
    Json,
}

/// The user's colour override, before anstream applies the environment.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ColorPolicy {
    /// Let anstream decide from the stream and the environment.
    #[default]
    Auto,
    /// Emit ANSI even into a pipe — how the demo transcript is captured.
    Always,
    /// Strip ANSI even on a terminal.
    Never,
}

impl ColorPolicy {
    /// Translate to anstream's own choice.
    #[must_use]
    pub fn choice(self) -> ColorChoice {
        match self {
            ColorPolicy::Auto => ColorChoice::Auto,
            // `AlwaysAnsi` rather than `Always`: on Windows, `Always` may use
            // the console API instead of emitting sequences, and a captured
            // transcript needs the sequences themselves.
            ColorPolicy::Always => ColorChoice::AlwaysAnsi,
            ColorPolicy::Never => ColorChoice::Never,
        }
    }
}

/// The decision, and the reason for it.
///
/// The reason is carried rather than discarded so the demo can SHOW the
/// strategy working: a prototype whose whole point is "auto-detect" has to be
/// able to say what it detected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Presentation {
    /// Rich or JSON.
    pub mode: Mode,
    /// Why, in one human clause.
    pub reason: &'static str,
    /// The colour override to hand anstream.
    pub color: ColorPolicy,
}

impl Presentation {
    /// Decide from the flags and the stream.
    ///
    /// `stdout_is_terminal` is a parameter rather than a call to
    /// [`IsTerminal`] so the decision table is testable without a pty; see
    /// [`Presentation::for_stdout`] for the real wiring.
    #[must_use]
    pub fn decide(
        force_json: bool,
        force_rich: bool,
        color: ColorPolicy,
        stdout_is_terminal: bool,
    ) -> Self {
        // `--json` wins over `--human`: the machine-readable shape is the one a
        // script depends on, so the flag that guarantees it is never overridden
        // by a flag that prettifies.
        if force_json {
            return Presentation {
                mode: Mode::Json,
                reason: "--json was given",
                color,
            };
        }
        if force_rich {
            return Presentation {
                mode: Mode::Rich,
                reason: "--human was given",
                color,
            };
        }
        if stdout_is_terminal {
            Presentation {
                mode: Mode::Rich,
                reason: "stdout is a terminal",
                color,
            }
        } else {
            Presentation {
                mode: Mode::Json,
                reason: "stdout is not a terminal — something is consuming this",
                color,
            }
        }
    }

    /// The same decision, against the real stdout.
    #[must_use]
    pub fn for_stdout(force_json: bool, force_rich: bool, color: ColorPolicy) -> Self {
        Self::decide(
            force_json,
            force_rich,
            color,
            std::io::stdout().is_terminal(),
        )
    }

    /// A stdout wrapper that applies the colour decision at write time.
    #[must_use]
    pub fn stream(&self) -> AutoStream<Stdout> {
        AutoStream::new(std::io::stdout(), self.color.choice())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_terminal_gets_rich_and_a_pipe_gets_json() {
        assert_eq!(
            Presentation::decide(false, false, ColorPolicy::Auto, true).mode,
            Mode::Rich
        );
        assert_eq!(
            Presentation::decide(false, false, ColorPolicy::Auto, false).mode,
            Mode::Json
        );
    }

    #[test]
    fn json_wins_over_human_and_over_the_terminal() {
        let both = Presentation::decide(true, true, ColorPolicy::Auto, true);
        assert_eq!(both.mode, Mode::Json);
        assert_eq!(both.reason, "--json was given");
    }

    #[test]
    fn human_forces_rich_into_a_pipe() {
        let piped = Presentation::decide(false, true, ColorPolicy::Auto, false);
        assert_eq!(piped.mode, Mode::Rich);
    }

    #[test]
    fn colour_is_independent_of_shape() {
        // Rich-into-a-pipe with colour forced is exactly how the demo
        // transcript is captured; the shape decision must not touch it.
        let captured = Presentation::decide(false, true, ColorPolicy::Always, false);
        assert_eq!(captured.mode, Mode::Rich);
        assert_eq!(captured.color.choice(), ColorChoice::AlwaysAnsi);
    }
}
