use std::collections::BTreeMap;

use comfy_table::Cell;
use fs3_core::{
    envelope::Envelope,
    views::status::{QueueRow, StatusReport},
};
use owo_colors::OwoColorize;
use serde_json::Value;

use crate::render::theme;

#[must_use]
pub fn render(envelope: &Envelope<Value>, width: u16) -> Option<String> {
    let report: StatusReport = serde_json::from_value(envelope.data.clone()?).ok()?;
    let files: i64 = report.roots.iter().map(|root| root.files).sum();
    let queued: i64 = report
        .queue
        .iter()
        .filter(|row| matches!(row.state.as_str(), "pending" | "running"))
        .map(|row| row.count)
        .sum();
    let failed: i64 = report
        .queue
        .iter()
        .filter(|row| row.state == "failed")
        .map(|row| row.count)
        .sum();
    let mut summary = format!(
        "{} roots · {files} files · {queued} queued",
        report.roots.len()
    );
    if failed > 0 {
        summary.push_str(&format!(" · {failed} failed"));
    }
    let mut out = theme::title("status", &summary);
    out.push_str("\n\n");

    if report.roots.is_empty() {
        out.push_str(&format!(
            "{}{}\n",
            theme::GUTTER,
            "no roots registered — `flowspace3 add <path>` to index one".bright_black()
        ));
    } else {
        let mut roots = theme::table(width);
        roots.set_header([
            theme::header("repo"),
            theme::header("root"),
            theme::header("files"),
        ]);
        for root in &report.roots {
            roots.add_row([
                Cell::new(format!("{}", root.identity.bright_white())),
                Cell::new(format!("{}", root.root_path.bright_black())),
                theme::right(theme::count(root.files)),
            ]);
        }
        out.push_str(&theme::block(&roots));
    }

    if !report.queue.is_empty() {
        let mut grouped: BTreeMap<&str, BTreeMap<&str, &QueueRow>> = BTreeMap::new();
        for row in &report.queue {
            grouped
                .entry(&row.kind)
                .or_default()
                .insert(&row.state, row);
        }
        let mut queue = theme::table(width);
        queue.set_header([
            theme::header("job"),
            theme::header("progress"),
            theme::header("done"),
            theme::header("running"),
            theme::header("pending"),
            theme::header("failed"),
        ]);
        for (kind, states) in grouped {
            let count = |state: &str| states.get(state).map_or(0, |row| row.count);
            let done = count("done");
            let running = count("running");
            let pending = count("pending");
            let failed = count("failed");
            queue.add_row([
                Cell::new(format!("{}", kind.bright_white())),
                Cell::new(theme::meter(done, done + running + pending + failed, 12)),
                theme::right(theme::count(done)),
                theme::right(theme::count(running)),
                theme::right(theme::count(pending)),
                theme::right(theme::count(failed)),
            ]);
        }
        out.push('\n');
        out.push_str(&theme::block(&queue));
    }
    for issue in &report.inconsistencies {
        out.push_str(&format!(
            "\n{}{} blob {} ({}) has roots: {}\n{}  {}\n",
            theme::GUTTER,
            "data inconsistency".bright_red(),
            issue.blob_sha,
            issue.parser_version,
            issue.paths.join(", "),
            theme::GUTTER,
            issue.next_action.bright_black(),
        ));
    }
    if !report.schema_ahead.is_empty() {
        out.push_str(&format!(
            "\n{}{} database schema is ahead: {:?}\n",
            theme::GUTTER,
            "!".bright_red(),
            report.schema_ahead
        ));
    }
    if let Some(last) = &report.last_error {
        out.push_str(&format!(
            "\n{}{} {}\n{}  {}\n",
            theme::GUTTER,
            "last error".bright_black(),
            last.job,
            theme::GUTTER,
            last.error.bright_red()
        ));
    }
    if let Some(next) = &envelope.next_action {
        out.push('\n');
        out.push_str(&theme::next_action(next, usize::from(width)));
        out.push('\n');
    }
    Some(out)
}
