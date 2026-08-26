//! `doctor` — the found→did checklist.
//!
//! # Why `repaired` is not a green tick
//!
//! Doctor repairs as it goes, so most of its rows describe a CHANGE to the
//! reader's machine. Rendering those the same as "was already fine" would hide
//! the most important fact on the screen. Three outcomes, three glyphs, three
//! colours: `✓` green was fine, `✚` yellow doctor changed something, `✗` red
//! doctor could not.
//!
//! # Why `found` and `action` are stacked, not columned
//!
//! They are a sentence: *found this, so did that*. Side by side they read as
//! two independent facts and the causal link is lost — and the second column
//! would be empty for every row that needed no repair, which is the majority.

use comfy_table::Cell;
use fs3_core::envelope::Envelope;
use owo_colors::{OwoColorize, Style};
use serde_json::Value;

use crate::render::RenderOptions;
use crate::surfaces::generic;
use crate::theme::{self, GUTTER};
use crate::views::{DoctorReport, Step};

/// Render a `doctor` envelope.
#[must_use]
pub fn render(envelope: &Envelope<Value>, options: &RenderOptions) -> String {
    let Some(report) = envelope
        .data
        .clone()
        .and_then(|data| serde_json::from_value::<DoctorReport>(data).ok())
    else {
        return generic::render(envelope, options);
    };

    let mut out = String::new();
    out.push_str(&theme::title("doctor", &summary(&report)));
    out.push_str("\n\n");
    out.push_str(&checklist(&report.steps, options));

    // A failed step is the only thing on this screen a reader must act on, so
    // it gets repeated at the bottom where the eye ends up.
    let blocked: Vec<&Step> = report
        .steps
        .iter()
        .filter(|step| step.outcome == "failed")
        .collect();
    if !blocked.is_empty() {
        out.push('\n');
        for step in blocked {
            let action = step.action.as_deref().unwrap_or("no automatic repair");
            out.push_str(&format!(
                "{GUTTER}{} {} {}\n",
                "✗".bright_red(),
                step.check.bright_red().bold(),
                action.bright_white()
            ));
        }
    }

    if let Some(next) = &envelope.next_action {
        out.push('\n');
        out.push_str(&theme::next_action(next, options.width as usize));
        out.push('\n');
    }
    out
}

/// `store healthy · 3 repaired · 1 failed`.
///
/// `healthy` and `failed` are independent by design: doctor can leave the store
/// perfectly usable while failing a check that is not on the store's path (a
/// missing embedder credential blocks enrichment, not queries). The header says
/// both rather than collapsing them into a single traffic light that would be
/// wrong in one direction or the other.
fn summary(report: &DoctorReport) -> String {
    let repaired = report
        .steps
        .iter()
        .filter(|step| step.outcome == "repaired")
        .count();
    let failed = report
        .steps
        .iter()
        .filter(|step| step.outcome == "failed")
        .count();

    let mut parts = vec![if report.healthy {
        format!("{}", "store healthy".green())
    } else {
        format!("{}", "store NOT usable".bright_red().bold())
    }];
    if repaired > 0 {
        parts.push(format!(
            "{}",
            format!("{repaired} repaired").bright_yellow()
        ));
    }
    if failed > 0 {
        parts.push(format!("{}", format!("{failed} failed").bright_red()));
    }
    let elapsed: u128 = report.steps.iter().filter_map(|step| step.elapsed_ms).sum();
    if elapsed > 0 {
        parts.push(elapsed_label(elapsed));
    }
    parts.join(&format!("{}", " · ".bright_black()))
}

/// One row per step, borderless.
fn checklist(steps: &[Step], options: &RenderOptions) -> String {
    let mut table = theme::plain_table(options.width);
    for step in steps {
        table.add_row(vec![
            Cell::new(theme::outcome_glyph(&step.outcome)),
            Cell::new(format!("{}", step.check.bright_white())),
            Cell::new(theme::outcome_word(&step.outcome)),
            Cell::new(detail(step)),
            theme::right(
                step.elapsed_ms
                    .map(elapsed_label)
                    .unwrap_or_else(|| format!("{}", "—".bright_black())),
            ),
        ]);
    }
    theme::block(&table)
}

/// What was found, and — indented under it — what was done about it.
///
/// A FAILED step's `action` is deliberately left out here: it is the one thing
/// on the screen the reader must act on, so it is restated below the table
/// where the eye ends up. Printing it in both places was noise, and the
/// duplicate read as two separate problems.
fn detail(step: &Step) -> String {
    // `found` and `action` both name commands and identifiers in backticks —
    // the same convention the catalog's `fix` uses — so they get the same
    // treatment: the punctuation becomes colour.
    let mut cell = theme::spans(
        &step.found,
        Style::new().bright_black(),
        Style::new().cyan(),
    );
    if step.outcome != "failed"
        && let Some(action) = &step.action
    {
        cell.push_str(&format!(
            "\n{} {}",
            "→".bright_yellow(),
            theme::spans(
                action,
                Style::new().bright_white(),
                Style::new().bright_cyan()
            )
        ));
    }
    cell
}

/// `61ms` under a second, `4.2s` over it — a reader comparing steps wants the
/// same unit only while it stays readable.
fn elapsed_label(ms: u128) -> String {
    let text = if ms < 1000 {
        format!("{ms}ms")
    } else {
        #[expect(
            clippy::cast_precision_loss,
            reason = "wall-clock milliseconds rendered to one decimal place"
        )]
        let seconds = ms as f64 / 1000.0;
        format!("{seconds:.1}s")
    };
    // Slow steps are worth noticing: a doctor run that took eight seconds spent
    // it somewhere, and that somewhere is usually the interesting part.
    if ms >= 2000 {
        format!("{}", text.bright_yellow())
    } else {
        format!("{}", text.bright_black())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render;

    fn fixture() -> Envelope<Value> {
        let bytes = include_bytes!("../../fixtures/doctor.json");
        serde_json::from_slice(bytes).expect("the doctor fixture is a valid envelope")
    }

    fn plain(styled: &str) -> String {
        anstream::adapter::strip_str(styled).to_string()
    }

    #[test]
    fn every_step_is_a_row_with_its_found_and_its_did() {
        let screen = plain(&render::render(&fixture(), &RenderOptions::default()));
        for check in ["engine", "stack", "database", "schema", "extension"] {
            assert!(screen.contains(check), "missing {check}:\n{screen}");
        }
        assert!(screen.contains("CREATE DATABASE flowspace3"), "{screen}");
        assert!(screen.contains("exited (137)"), "{screen}");
    }

    #[test]
    fn the_three_outcomes_get_three_glyphs() {
        let screen = plain(&render::render(&fixture(), &RenderOptions::default()));
        assert!(screen.contains('✓'), "ok glyph missing:\n{screen}");
        assert!(screen.contains('✚'), "repaired glyph missing:\n{screen}");
        assert!(screen.contains('✗'), "failed glyph missing:\n{screen}");
    }

    #[test]
    fn a_failed_step_is_repeated_at_the_bottom() {
        let screen = plain(&render::render(&fixture(), &RenderOptions::default()));
        let occurrences = screen.matches("embedder").count();
        assert!(
            occurrences >= 2,
            "the failed check must be restated below the table:\n{screen}"
        );
    }

    #[test]
    fn healthy_and_failed_are_reported_independently() {
        // The fixture is the awkward, real case: healthy store, failed check.
        let screen = plain(&render::render(&fixture(), &RenderOptions::default()));
        assert!(screen.contains("store healthy"), "{screen}");
        assert!(screen.contains("1 failed"), "{screen}");
    }

    #[test]
    fn an_unhealthy_store_says_so_in_the_title() {
        let sick: Envelope<Value> = serde_json::from_str(
            r#"{"ok":true,"command":"doctor","v":1,
                "data":{"healthy":false,"steps":[
                  {"check":"engine","outcome":"failed","found":"no docker socket",
                   "action":"install Docker Desktop or start colima"}]}}"#,
        )
        .unwrap();
        let screen = plain(&render::render(&sick, &RenderOptions::default()));
        assert!(screen.contains("store NOT usable"), "{screen}");
    }

    #[test]
    fn elapsed_switches_unit_at_a_second() {
        assert_eq!(plain(&elapsed_label(61)), "61ms");
        assert_eq!(plain(&elapsed_label(4213)), "4.2s");
    }
}
