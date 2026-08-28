use comfy_table::Cell;
use fs3_core::{
    envelope::Envelope,
    views::remove::{GcCounts, RemoveReport},
};
use owo_colors::OwoColorize;
use serde_json::Value;

use crate::render::theme;

#[must_use]
pub fn remove(envelope: &Envelope<Value>, width: u16) -> Option<String> {
    let report: RemoveReport = serde_json::from_value(envelope.data.clone()?).ok()?;
    let summary = if report.was_registered {
        format!("{} files unregistered", report.files)
    } else {
        "not registered".to_string()
    };
    let mut out = theme::title("remove", &summary);
    out.push_str("\n\n");
    let mut facts = theme::plain_table(width);
    facts.add_row([Cell::new("root"), Cell::new(report.root_path)]);
    if let Some(identity) = report.identity {
        facts.add_row([Cell::new("repository"), Cell::new(identity)]);
    }
    facts.add_row([Cell::new("jobs killed"), theme::right(report.jobs_killed)]);
    facts.add_row([
        Cell::new("reclaimable rows"),
        theme::right(report.reclaimable.total),
    ]);
    out.push_str(&theme::block(&facts));
    append_next(&mut out, envelope, width);
    Some(out)
}

#[must_use]
pub fn gc(envelope: &Envelope<Value>, width: u16) -> Option<String> {
    let report: GcCounts = serde_json::from_value(envelope.data.clone()?).ok()?;
    let mut out = theme::title("gc", &format!("{} rows reclaimed", report.total));
    out.push_str("\n\n");
    let mut table = theme::plain_table(width);
    for (label, value) in [
        ("jobs", report.jobs),
        ("elements", report.elements),
        ("summaries", report.summaries),
        ("embeddings", report.embeddings),
    ] {
        table.add_row([
            Cell::new(format!("{}", label.bright_black())),
            theme::right(value),
        ]);
    }
    out.push_str(&theme::block(&table));
    append_next(&mut out, envelope, width);
    Some(out)
}

fn append_next(out: &mut String, envelope: &Envelope<Value>, width: u16) {
    if let Some(next) = &envelope.next_action {
        out.push('\n');
        out.push_str(&theme::next_action(next, usize::from(width)));
        out.push('\n');
    }
}
