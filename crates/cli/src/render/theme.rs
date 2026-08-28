//! Shared visual vocabulary for every human-rendered envelope.

use comfy_table::{Cell, CellAlignment, ContentArrangement, Table, presets};
use owo_colors::{OwoColorize, Style};

pub const GUTTER: &str = "  ";
const TITLE_BAR: &str = "▍";
const METER_FULL: char = '█';
const METER_EMPTY: char = '░';

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

#[must_use]
pub fn spans(text: &str, outer: Style, inner: Style) -> String {
    text.split('`')
        .enumerate()
        .map(|(index, segment)| {
            if index % 2 == 1 {
                format!("{}", segment.style(inner))
            } else {
                format!("{}", segment.style(outer))
            }
        })
        .collect()
}

#[must_use]
pub fn fix_text(fix: &str) -> String {
    spans(
        fix,
        Style::new().bright_white(),
        Style::new().bright_cyan().bold(),
    )
}

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

#[must_use]
pub fn wrap(text: &str, width: usize, hang: usize) -> String {
    let indent = " ".repeat(hang);
    textwrap::fill(
        text,
        textwrap::Options::new(width.max(20))
            .initial_indent("")
            .subsequent_indent(&indent),
    )
}

#[must_use]
pub fn score_meter(score: f64, cells: usize) -> String {
    let clamped = score.clamp(0.0, 1.0);
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "score is clamped and scaled by a small display width"
    )]
    let filled = ((clamped * cells as f64).round() as usize).min(cells);
    let meter: String = std::iter::repeat_n(METER_FULL, filled)
        .chain(std::iter::repeat_n(METER_EMPTY, cells - filled))
        .collect();
    let number = format!("{clamped:.2}");
    match clamped {
        value if value >= 0.75 => format!("{} {}", number.bright_green(), meter.green()),
        value if value >= 0.50 => format!("{} {}", number.bright_yellow(), meter.yellow()),
        _ => format!("{} {}", number.bright_black(), meter.bright_black()),
    }
}

#[must_use]
pub fn meter(done: i64, total: i64, cells: usize) -> String {
    if total <= 0 {
        return " ".repeat(cells);
    }
    #[expect(clippy::cast_precision_loss, reason = "display-scale counts")]
    let fraction = (done as f64 / total as f64).clamp(0.0, 1.0);
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "fraction is clamped and scaled by a small display width"
    )]
    let filled = ((fraction * cells as f64).round() as usize).min(cells);
    let bar: String = std::iter::repeat_n(METER_FULL, filled).collect();
    let rest: String = std::iter::repeat_n(METER_EMPTY, cells - filled).collect();
    format!("{}{}", bar.cyan(), rest.bright_black())
}

#[must_use]
pub fn outcome_glyph(outcome: &str) -> String {
    match outcome {
        "ok" => format!("{}", "✓".green()),
        "repaired" => format!("{}", "✚".bright_yellow()),
        "warn" => format!("{}", "!".bright_yellow()),
        "down" => format!("{}", "✗".bright_red()),
        "info" => format!("{}", "·".bright_cyan()),
        _ => format!("{}", "·".bright_black()),
    }
}

#[must_use]
pub fn count(count: i64) -> String {
    if count == 0 {
        format!("{}", "0".bright_black())
    } else {
        count.to_string()
    }
}

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

fn inner_width(width: u16) -> u16 {
    width
        .saturating_sub(u16::try_from(GUTTER.len()).unwrap_or(2))
        .max(20)
}

#[must_use]
pub fn block(table: &Table) -> String {
    let mut out = String::new();
    for line in table.to_string().lines() {
        out.push_str(GUTTER);
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}

#[must_use]
pub fn header(text: &str) -> Cell {
    Cell::new(format!("{}", text.bright_black()))
}

#[must_use]
pub fn right(text: impl std::fmt::Display) -> Cell {
    Cell::new(text.to_string()).set_alignment(CellAlignment::Right)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(styled: &str) -> String {
        anstream::adapter::strip_str(styled).to_string()
    }

    #[test]
    fn tables_never_sniff_the_terminal() {
        let mut value = table(80);
        value.add_row(["hello"]);
        assert!(value.to_string().contains("hello"));
    }

    #[test]
    fn nested_command_spans_keep_the_outer_style_segments() {
        let text = spans(
            "run `flowspace3 doctor` now",
            Style::new().dimmed(),
            Style::new().cyan(),
        );
        assert_eq!(plain(&text), "run flowspace3 doctor now");
    }

    #[test]
    fn meter_clamps_future_scores() {
        assert_eq!(plain(&score_meter(3.4, 4)), "1.00 ████");
    }
}
