//! `demo` — the prototype's showroom and its TTY strategy, in one binary.
//!
//! ```console
//! $ cargo run --bin demo                 # rich: you are at a terminal
//! $ cargo run --bin demo | jq .          # JSON: something is consuming this
//! $ cargo run --bin demo -- --json       # JSON, always
//! $ cargo run --bin demo -- --human --color always > transcript.ansi
//! ```
//!
//! The binary is deliberately thin: it decides the presentation, reads bytes,
//! calls [`human_render::render`], and writes. Every judgement about what a
//! screen should look like lives in the library, which is what makes the
//! promotion path to `fs3-cli` a move rather than a rewrite.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, ValueEnum};
use human_render::{ColorPolicy, Mode, Presentation, RenderOptions, render};
use owo_colors::OwoColorize;

/// The fixtures, embedded so the demo runs from any directory.
const FIXTURES: [(&str, &str); 4] = [
    ("search", include_str!("../../fixtures/search.json")),
    ("doctor", include_str!("../../fixtures/doctor.json")),
    ("error", include_str!("../../fixtures/error.json")),
    ("status", include_str!("../../fixtures/status.json")),
];

/// Prototype: the human skin over the frozen fs3 envelope.
#[derive(Debug, Parser)]
#[command(name = "demo", about, long_about = None)]
struct Cli {
    /// Which surface to show.
    #[arg(long, value_enum, default_value_t = Surface::All)]
    surface: Surface,

    /// Render an envelope from a file (or `-` for stdin) instead of a fixture.
    #[arg(long, value_name = "PATH")]
    file: Option<PathBuf>,

    /// Force the machine shape: print the envelope, verbatim.
    #[arg(long)]
    json: bool,

    /// Force the human shape even when stdout is not a terminal.
    #[arg(long)]
    human: bool,

    /// Colour override; `auto` asks the environment (NO_COLOR, TERM, …).
    #[arg(long, value_enum, default_value_t = Color::Auto)]
    color: Color,

    /// Canvas width. Defaults to the terminal's, or 100 when there isn't one.
    #[arg(long)]
    width: Option<u16>,

    /// Print which presentation was chosen, and why, before the output.
    #[arg(long)]
    explain: bool,
}

/// Which of the four screens to draw.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum Surface {
    /// All four, in order.
    All,
    /// Ranked results plus the folder steer.
    Search,
    /// The found→did checklist.
    Doctor,
    /// A failure, with the fix made primary.
    Error,
    /// Roots and queue depth.
    Status,
}

/// The `--color` flag, mapped to the library's policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum Color {
    /// Let the environment decide.
    Auto,
    /// Emit sequences even into a pipe.
    Always,
    /// Never emit sequences.
    Never,
}

impl From<Color> for ColorPolicy {
    fn from(color: Color) -> Self {
        match color {
            Color::Auto => ColorPolicy::Auto,
            Color::Always => ColorPolicy::Always,
            Color::Never => ColorPolicy::Never,
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let presentation = Presentation::for_stdout(cli.json, cli.human, cli.color.into());

    // The library never looks at the terminal; the binary does it once, here.
    let width = cli.width.unwrap_or_else(terminal_width);
    let options = RenderOptions::width(width);

    let mut out = presentation.stream();
    let sources = match load(&cli) {
        Ok(sources) => sources,
        Err(error) => {
            eprintln!("demo: {error}");
            return ExitCode::FAILURE;
        }
    };

    if cli.explain {
        let shape = match presentation.mode {
            Mode::Rich => "rich",
            Mode::Json => "json",
        };
        let _ = writeln!(
            out,
            "{} {} — {}\n",
            "presentation:".bright_black(),
            shape.bright_cyan(),
            presentation.reason.bright_black()
        );
    }

    // A failure envelope renders as a failure AND exits non-zero: the human
    // skin must not make a broken command look successful to the shell.
    let mut failed = false;

    for (name, bytes) in sources {
        match presentation.mode {
            // Verbatim: the JSON path must never be a re-serialisation, or the
            // two skins are no longer showing the same bytes.
            Mode::Json => {
                let _ = writeln!(out, "{}", bytes.trim_end());
            }
            Mode::Rich => match serde_json::from_str(&bytes) {
                Ok(envelope) => {
                    failed |= !fs3_ok(&envelope);
                    let _ = write!(out, "{}", render(&envelope, &options));
                    let _ = writeln!(out);
                }
                Err(error) => {
                    // Not an envelope at all. Say what arrived, and where from.
                    eprintln!("demo: {name} is not an envelope: {error}");
                    failed = true;
                }
            },
        }
    }
    let _ = out.flush();

    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// `ok` is the only discriminator, on this side of the wire too.
fn fs3_ok(envelope: &human_render::Envelope<serde_json::Value>) -> bool {
    envelope.ok
}

/// The bytes to render: a file, stdin, or the built-in fixtures.
fn load(cli: &Cli) -> Result<Vec<(String, String)>, String> {
    if let Some(path) = &cli.file {
        return read_source(path).map(|bytes| vec![(path.display().to_string(), bytes)]);
    }
    Ok(FIXTURES
        .iter()
        .filter(|(name, _)| match cli.surface {
            Surface::All => true,
            Surface::Search => *name == "search",
            Surface::Doctor => *name == "doctor",
            Surface::Error => *name == "error",
            Surface::Status => *name == "status",
        })
        .map(|(name, bytes)| ((*name).to_string(), (*bytes).to_string()))
        .collect())
}

/// Read one envelope from a path, or from stdin for `-`.
fn read_source(path: &Path) -> Result<String, String> {
    if path.as_os_str() == "-" {
        return std::io::read_to_string(std::io::stdin())
            .map_err(|error| format!("cannot read stdin: {error}"));
    }
    std::fs::read_to_string(path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))
}

/// The terminal's width, or the library default when there is no terminal.
///
/// This is the only place in the prototype that asks the operating system
/// anything about the display, which is the point: one boundary, one question.
fn terminal_width() -> u16 {
    let width = textwrap::termwidth();
    if width < 40 {
        human_render::render::DEFAULT_WIDTH
    } else {
        u16::try_from(width).unwrap_or(human_render::render::DEFAULT_WIDTH)
    }
}
