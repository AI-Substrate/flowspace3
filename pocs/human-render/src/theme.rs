//! The visual vocabulary: one palette, one set of glyphs, one table builder.
//!
//! Everything a surface draws goes through here, so the four surfaces cannot
//! drift into four dialects — and so a promotion to `fs3-cli` has exactly one
//! file to review for house style.
//!
//! # Palette
//!
//! The 16 ANSI colours and their bright variants only — no truecolour. A user's
//! terminal theme IS their palette; `#5f87af` looks deliberate on the author's
//! machine and unreadable on a solarised-light one. Bright black is the dim
//! channel, used for everything the eye should skip on the way to the answer.
//!
//! # Colour is emitted unconditionally
//!
//! Nothing here asks whether colour is wanted; see [`crate::mode`]. That is why
//! these helpers can be `const`-simple and why tests can assert against
//! `anstream::adapter::strip_str` output.

use comfy_table::{Cell, CellAlignment, ContentArrangement, Table, presets};
use owo_colors::{OwoColorize, Style};

/// Left gutter for every non-table line, so surfaces line up with each other.
pub const GUTTER: &str = "  ";

/// The heavy bar that opens a section title.
const TITLE_BAR: &str = "▍";

/// Meter cells, from empty to full.
const METER_FULL: char = '█';
/// The unfilled remainder of a meter.
const METER_EMPTY: char = '░';

/// A section title: `▍ search` plus a dim one-line summary.
///
/// The command comes from the envelope's own `command` field, never from the
/// renderer's idea of what it was asked — an envelope replayed from a log
/// renders as the verb it actually answered.
#[must_use]
pub fn title(command: &str, summary: &str) -> String {
    let bar = TITLE_BAR.bright_cyan();
    let verb = format!("{}", command.bold().bright_white());
    if summary.is_empty() {
        format!("{GUTTER}{bar} {verb}")
    } else {
        format!("{GUTTER}{bar} {verb}  {}", summary.bright_black())
    }
}

/// Style the backticked spans in `text` and drop the backticks.
///
/// Every fs3 `fix` and `next_action` names a command in backticks — that is the
/// convention the catalog is written in — so the renderer treats them as markup
/// and lets colour do what the punctuation was standing in for. The backticks
/// are removed rather than kept, because the text is now something a reader
/// double-clicks and pastes.
///
/// Both styles are applied per SEGMENT rather than nesting one inside the
/// other: `"a `b` c".bright_black()` would emit a reset at the end of the inner
/// span and lose the dim for `c`. Segments each open and close their own style,
/// so there is nothing to nest.
#[must_use]
pub fn spans(text: &str, outer: Style, inner: Style) -> String {
    text.split('`')
        .enumerate()
        .map(|(index, segment)| {
            // Odd segments are inside backticks — including an UNTERMINATED
            // trailing one, which is what a wrapped line looks like.
            if index % 2 == 1 {
                format!("{}", segment.style(inner))
            } else {
                format!("{}", segment.style(outer))
            }
        })
        .collect()
}

/// A `fix` line: bright, with its commands brighter.
#[must_use]
pub fn fix_text(fix: &str) -> String {
    spans(
        fix,
        Style::new().bright_white(),
        Style::new().bright_cyan().bold(),
    )
}

/// The agent steer (`next_action`, PRD req 44) as a human's next command.
///
/// Rendered as advice, never as an imperative the reader must obey — the
/// envelope is explicit that it is advice, and the styling should say so too:
/// one dim arrow, below the answer, out of the way. The command inside it is
/// still legible, because advice you cannot read is not advice.
#[must_use]
pub fn next_action(next: &str, width: usize) -> String {
    let arrow = "→".bright_cyan();
    let body = wrap(next, width.saturating_sub(GUTTER.len() + 2), 0);
    let outer = Style::new().bright_black();
    let inner = Style::new().cyan();
    let mut lines = body.lines();
    let first = lines.next().unwrap_or_default();
    let mut out = format!("{GUTTER}{arrow} {}", spans(first, outer, inner));
    for line in lines {
        out.push('\n');
        out.push_str(&format!("{GUTTER}  {}", spans(line, outer, inner)));
    }
    out
}

/// `key: value`, with the key dim and the value plain.
#[must_use]
pub fn kv(key: &str, value: &str) -> String {
    format!("{}{} {}", GUTTER, format!("{key}:").bright_black(), value)
}

/// Wrap prose to `width`, indenting every line after the first by `hang`.
#[must_use]
pub fn wrap(text: &str, width: usize, hang: usize) -> String {
    let width = width.max(20);
    let indent = " ".repeat(hang);
    let options = textwrap::Options::new(width)
        .initial_indent("")
        .subsequent_indent(&indent);
    textwrap::fill(text, options)
}

/// A score meter: `0.83 ████████░░`.
///
/// Colour bands rather than a gradient, because the question a reader has is
/// "is this a real hit?" — a three-way answer (strong / plausible / noise) is
/// one glance, and 256 shades of green are none.
#[must_use]
pub fn score_meter(score: f64, cells: usize) -> String {
    let clamped = score.clamp(0.0, 1.0);
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "clamped to 0.0..=1.0 then scaled by a small cell count"
    )]
    let filled = (clamped * cells as f64).round() as usize;
    let filled = filled.min(cells);
    let meter: String = std::iter::repeat_n(METER_FULL, filled)
        .chain(std::iter::repeat_n(METER_EMPTY, cells - filled))
        .collect();
    let number = format!("{clamped:.2}");
    match clamped {
        s if s >= 0.75 => format!("{} {}", number.bright_green(), meter.green()),
        s if s >= 0.50 => format!("{} {}", number.bright_yellow(), meter.yellow()),
        _ => format!("{} {}", number.bright_black(), meter.bright_black()),
    }
}

/// A proportion meter with no number, for queue depth.
#[must_use]
pub fn meter(done: i64, total: i64, cells: usize) -> String {
    if total <= 0 {
        return " ".repeat(cells);
    }
    #[expect(
        clippy::cast_precision_loss,
        reason = "counts are display-scale; a rounding error of one cell is invisible"
    )]
    let fraction = (done as f64 / total as f64).clamp(0.0, 1.0);
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "fraction is clamped to 0.0..=1.0"
    )]
    let filled = ((fraction * cells as f64).round() as usize).min(cells);
    let bar: String = std::iter::repeat_n(METER_FULL, filled).collect();
    let rest: String = std::iter::repeat_n(METER_EMPTY, cells - filled).collect();
    format!("{}{}", bar.cyan(), rest.bright_black())
}

/// The three outcomes a checklist row can have, with their glyph and colour.
///
/// `repaired` gets its own glyph rather than a green tick: doctor CHANGED
/// something, and a reader who scrolls past that has been misinformed by the
/// rendering.
#[must_use]
pub fn outcome_glyph(outcome: &str) -> String {
    match outcome {
        "ok" => format!("{}", "✓".green()),
        "repaired" => format!("{}", "✚".bright_yellow()),
        "failed" => format!("{}", "✗".bright_red()),
        _ => format!("{}", "·".bright_black()),
    }
}

/// The same three outcomes as a word, for the checklist's second column.
#[must_use]
pub fn outcome_word(outcome: &str) -> String {
    match outcome {
        "ok" => format!("{}", outcome.green()),
        "repaired" => format!("{}", outcome.bright_yellow()),
        "failed" => format!("{}", outcome.bright_red().bold()),
        other => format!("{}", other.bright_black()),
    }
}

/// A count that means nothing when zero and something when not.
#[must_use]
pub fn count_or_dim(count: i64, style: Emphasis) -> String {
    if count == 0 {
        return format!("{}", "0".bright_black());
    }
    match style {
        Emphasis::Good => format!("{}", count.green()),
        Emphasis::Warn => format!("{}", count.bright_yellow()),
        Emphasis::Bad => format!("{}", count.bright_red()),
        Emphasis::Neutral => count.to_string(),
    }
}

/// How a non-zero number should read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Emphasis {
    /// Progress: work that completed.
    Good,
    /// Work outstanding, or a soft-failure count.
    Warn,
    /// Failure.
    Bad,
    /// Just a number.
    Neutral,
}

/// A bordered table, sized to `width`.
///
/// `ContentArrangement::Dynamic` is what makes the element column give way on a
/// narrow terminal instead of the table blowing past the edge and wrapping into
/// nonsense. The width is passed in rather than sniffed: this crate does not
/// look at the terminal (see [`crate::mode`]).
///
/// `force_no_tty` makes that literal. comfy-table's `custom_styling` feature
/// pulls in `tty`, which would otherwise let comfy-table sniff the terminal and
/// decide about styling on its own — a second opinion, invisible from here,
/// that could disagree with anstream's. Turning it off means the ONLY styling
/// on screen is the styling the surfaces put in the cells, and a table renders
/// identically in a pipe and on a terminal.
#[must_use]
pub fn table(width: u16) -> Table {
    let mut table = Table::new();
    table
        .load_style(presets::UTF8_FULL_CONDENSED.with_rounded_corners())
        .force_no_tty()
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_width(inner_width(width));
    table
}

/// A borderless table, for checklists and key/value blocks.
#[must_use]
pub fn plain_table(width: u16) -> Table {
    let mut table = Table::new();
    table
        .load_style(presets::NOTHING)
        .force_no_tty()
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_width(inner_width(width));
    table
}

/// The width a table gets once the gutter has taken its share.
fn inner_width(width: u16) -> u16 {
    let gutter = u16::try_from(GUTTER.len()).unwrap_or(2);
    width.saturating_sub(gutter).max(20)
}

/// A rendered table, pushed in under the gutter so it lines up with the prose.
///
/// comfy-table has no concept of a left margin, and a table flush against
/// column zero beside text that starts at column two reads as two documents.
#[must_use]
pub fn block(table: &Table) -> String {
    let mut out = String::new();
    for line in table.to_string().lines() {
        // `trim_end` matters for the borderless tables: comfy-table pads every
        // cell to its column width, which leaves a ragged tail of spaces in a
        // captured transcript and in a diff.
        out.push_str(GUTTER);
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}

/// A header cell: dim, so the data is what the eye lands on.
#[must_use]
pub fn header_cell(text: &str) -> Cell {
    Cell::new(format!("{}", text.bright_black()))
}

/// A right-aligned cell, for numbers.
#[must_use]
pub fn right(text: impl Into<String>) -> Cell {
    Cell::new(text.into()).set_alignment(CellAlignment::Right)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(styled: &str) -> String {
        anstream::adapter::strip_str(styled).to_string()
    }

    #[test]
    fn score_meter_fills_proportionally() {
        assert_eq!(plain(&score_meter(1.0, 10)), "1.00 ██████████");
        assert_eq!(plain(&score_meter(0.5, 10)), "0.50 █████░░░░░");
        assert_eq!(plain(&score_meter(0.0, 10)), "0.00 ░░░░░░░░░░");
    }

    #[test]
    fn score_meter_survives_an_out_of_range_score() {
        // A newer daemon could report a fused RRF score above 1.0; a renderer
        // that panicked on it would take the whole answer down.
        assert_eq!(plain(&score_meter(3.4, 4)), "1.00 ████");
        assert_eq!(plain(&score_meter(-2.0, 4)), "0.00 ░░░░");
    }

    #[test]
    fn meter_of_an_empty_queue_is_blank_not_full() {
        assert_eq!(plain(&meter(0, 0, 6)), "      ");
    }

    #[test]
    fn an_unknown_outcome_renders_neutrally_rather_than_lying() {
        assert_eq!(plain(&outcome_glyph("quarantined")), "·");
        assert_eq!(plain(&outcome_word("quarantined")), "quarantined");
    }

    #[test]
    fn wrap_hangs_continuation_lines() {
        let text = wrap("alpha beta gamma delta epsilon zeta", 20, 4);
        let lines: Vec<&str> = text.lines().collect();
        assert!(lines.len() > 1, "expected a wrap at width 20: {text:?}");
        assert!(lines[1].starts_with("    "), "expected a hanging indent");
    }
}
