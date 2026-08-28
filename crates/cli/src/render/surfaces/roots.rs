use comfy_table::Cell;
use fs3_core::{envelope::Envelope, views::roots::RootReport};
use owo_colors::OwoColorize;
use serde_json::Value;

use crate::render::theme;

#[must_use]
pub fn render(envelope: &Envelope<Value>, width: u16) -> Option<String> {
    let report: RootReport = serde_json::from_value(envelope.data.clone()?).ok()?;
    let mut out = theme::title(
        &envelope.command,
        &format!(
            "{} files · {} queued · {} unchanged",
            report.files, report.enqueued, report.unchanged
        ),
    );
    out.push_str("\n\n");
    let mut facts = theme::plain_table(width);
    facts.add_row([
        Cell::new("repository"),
        Cell::new(format!("{}", report.identity.bright_white())),
    ]);
    facts.add_row([Cell::new("root"), Cell::new(report.root_path)]);
    facts.add_row([Cell::new("identity"), Cell::new(report.identity_source)]);
    facts.add_row([Cell::new("removed"), Cell::new(report.removed)]);
    out.push_str(&theme::block(&facts));
    if !report.skipped.is_empty() {
        out.push_str(&format!(
            "\n{}{}\n",
            theme::GUTTER,
            "skipped".bright_black()
        ));
        let mut skipped = theme::plain_table(width);
        for row in report.skipped {
            skipped.add_row([Cell::new(row.reason), theme::right(row.count)]);
        }
        out.push_str(&theme::block(&skipped));
    }
    if !report.pruned.is_empty() {
        out.push_str(&format!(
            "\n{}{}\n",
            theme::GUTTER,
            "directories not walked".bright_black()
        ));
        let mut pruned = theme::plain_table(width);
        for row in report.pruned {
            pruned.add_row([Cell::new(row.path), Cell::new(row.fix)]);
        }
        out.push_str(&theme::block(&pruned));
    }
    if let Some(next) = &envelope.next_action {
        out.push('\n');
        out.push_str(&theme::next_action(next, usize::from(width)));
        out.push('\n');
    }
    Some(out)
}
