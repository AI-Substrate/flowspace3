use comfy_table::Cell;
use fs3_core::{envelope::Envelope, views::doctor::DoctorReport};
use owo_colors::{OwoColorize, Style};
use serde_json::Value;

use crate::render::{WIDTH, theme};

#[must_use]
pub fn render(envelope: &Envelope<Value>) -> Option<String> {
    let report: DoctorReport = serde_json::from_value(envelope.data.clone()?).ok()?;
    let repaired = report
        .steps
        .iter()
        .filter(|step| step.outcome == "repaired")
        .count();
    let blocked = report
        .steps
        .iter()
        .filter(|step| matches!(step.outcome.as_str(), "warn" | "down"))
        .count();
    let mut summary = report.verdict.clone();
    if repaired > 0 {
        summary.push_str(&format!(" · {repaired} repaired"));
    }
    if blocked > 0 {
        summary.push_str(&format!(" · {blocked} need attention"));
    }
    let mut out = theme::title("doctor", &summary);
    out.push_str("\n\n");
    let mut table = theme::plain_table(WIDTH);
    for step in &report.steps {
        let detail = theme::spans(
            &step.found,
            Style::new().bright_black(),
            Style::new().cyan(),
        );
        table.add_row([
            Cell::new(theme::outcome_glyph(&step.outcome)),
            Cell::new(format!("{}", step.check.bright_white())),
            Cell::new(step.outcome.clone()),
            Cell::new(detail),
            theme::right(format!("{}ms", step.elapsed_ms)),
        ]);
    }
    out.push_str(&theme::block(&table));
    for step in report
        .steps
        .iter()
        .filter(|step| matches!(step.outcome.as_str(), "warn" | "down"))
    {
        if let Some(action) = &step.action {
            out.push_str(&format!(
                "\n{}{} {}\n",
                theme::GUTTER,
                "→".bright_yellow(),
                theme::fix_text(action)
            ));
        }
    }
    if let Some(next) = &envelope.next_action {
        out.push('\n');
        out.push_str(&theme::next_action(next, usize::from(WIDTH)));
        out.push('\n');
    }
    Some(out)
}
