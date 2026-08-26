//! `status` — what is registered, and what is left to do.
//!
//! Two tables, because it is two questions that only make sense together: roots
//! without queue depth reads as done when nothing has started, and queue depth
//! without roots reads as broken when nothing was ever added.
//!
//! # The queue table is grouped by kind, not listed by state
//!
//! The daemon reports `(kind, state, count)` buckets. Rendering them as flat
//! rows makes the reader do the pivot in their head — eight rows to answer "is
//! summarize nearly done?". One row per kind with the states as columns and a
//! completion meter answers it at a glance, and the totals still add up because
//! nothing was dropped.

use std::collections::BTreeMap;

use comfy_table::Cell;
use fs3_core::envelope::Envelope;
use owo_colors::OwoColorize;
use serde_json::Value;

use crate::render::RenderOptions;
use crate::surfaces::generic;
use crate::theme::{self, Emphasis, GUTTER};
use crate::views::{QueueRow, Root, StatusReport};

/// Render a `status` envelope.
#[must_use]
pub fn render(envelope: &Envelope<Value>, options: &RenderOptions) -> String {
    let Some(report) = envelope
        .data
        .clone()
        .and_then(|data| serde_json::from_value::<StatusReport>(data).ok())
    else {
        return generic::render(envelope, options);
    };

    let mut out = String::new();
    out.push_str(&theme::title("status", &summary(&report)));
    out.push_str("\n\n");

    if report.roots.is_empty() {
        out.push_str(&format!(
            "{GUTTER}{}\n",
            "no roots registered — `flowspace3 add <path>` to index one".bright_black()
        ));
    } else {
        out.push_str(&roots_table(&report.roots, options));
        out.push('\n');
    }

    if !report.queue.is_empty() {
        out.push('\n');
        out.push_str(&queue_table(&report.queue, options));
        out.push('\n');
    }

    // A newer daemon has migrated this database past what this binary knows.
    // That is a real hazard (this build may write rows the other cannot read),
    // and it is the one thing on this screen that is not just a number.
    if !report.schema_ahead.is_empty() {
        let migrations = report
            .schema_ahead
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!(
            "\n{GUTTER}{} {}\n",
            "!".bright_red().bold(),
            format!("the database is AHEAD of this binary: migrations {migrations}").bright_red()
        ));
    }

    if let Some(last) = &report.last_error {
        out.push('\n');
        out.push_str(&format!(
            "{GUTTER}{} {}\n{GUTTER}  {}\n",
            "last error".bright_black(),
            last.job.bright_black(),
            last.error.bright_red()
        ));
    }

    if let Some(next) = &envelope.next_action {
        out.push('\n');
        out.push_str(&theme::next_action(next, options.width as usize));
        out.push('\n');
    }
    out
}

/// `3 roots · 1463 files · 591 queued · 7 failed`.
fn summary(report: &StatusReport) -> String {
    let roots = report.roots.len();
    let files: i64 = report.roots.iter().map(|root| root.files).sum();
    let queued: i64 = report
        .queue
        .iter()
        .filter(|row| row.state == "pending" || row.state == "running")
        .map(|row| row.count)
        .sum();
    let failed: i64 = report
        .queue
        .iter()
        .filter(|row| row.state == "failed")
        .map(|row| row.count)
        .sum();

    let mut parts = vec![
        format!("{roots} roots"),
        format!("{files} files"),
        if queued == 0 {
            format!("{}", "idle".green())
        } else {
            format!("{}", format!("{queued} queued").bright_cyan())
        },
    ];
    if failed > 0 {
        parts.push(format!("{}", format!("{failed} failed").bright_red()));
    }
    parts.join(&format!("{}", " · ".bright_black()))
}

/// Registered worktrees.
fn roots_table(roots: &[Root], options: &RenderOptions) -> String {
    let mut table = theme::table(options.width);
    table.set_header(vec![
        theme::header_cell("repo"),
        theme::header_cell("root"),
        theme::header_cell("files"),
    ]);
    for root in roots {
        table.add_row(vec![
            Cell::new(format!("{}", root.identity.bright_white())),
            Cell::new(format!("{}", root.root_path.bright_black())),
            theme::right(theme::count_or_dim(root.files, Emphasis::Neutral)),
        ]);
    }
    theme::block(&table)
}

/// The queue, pivoted: one row per kind, states as columns.
fn queue_table(rows: &[QueueRow], options: &RenderOptions) -> String {
    // BTreeMap for a stable order that does not depend on the daemon's grouping
    // — a status screen whose rows jump between refreshes is unreadable.
    let mut by_kind: BTreeMap<&str, BTreeMap<&str, QueueRow>> = BTreeMap::new();
    for row in rows {
        by_kind
            .entry(row.kind.as_str())
            .or_default()
            .insert(row.state.as_str(), row.clone());
    }

    let mut table = theme::table(options.width);
    table.set_header(vec![
        theme::header_cell("job"),
        theme::header_cell("progress"),
        theme::header_cell("done"),
        theme::header_cell("running"),
        theme::header_cell("pending"),
        theme::header_cell("failed"),
        theme::header_cell("with errors"),
    ]);

    for (kind, states) in by_kind {
        let count = |state: &str| states.get(state).map_or(0, |row| row.count);
        let done = count("done");
        let running = count("running");
        let pending = count("pending");
        let failed = count("failed");
        let with_error: i64 = states.values().map(|row| row.with_error).sum();
        let total = done + running + pending + failed;

        table.add_row(vec![
            Cell::new(format!("{}", kind.bright_white())),
            Cell::new(theme::meter(done, total, 14)),
            theme::right(theme::count_or_dim(done, Emphasis::Good)),
            theme::right(theme::count_or_dim(running, Emphasis::Neutral)),
            theme::right(theme::count_or_dim(pending, Emphasis::Warn)),
            theme::right(theme::count_or_dim(failed, Emphasis::Bad)),
            // Retried-then-succeeded jobs live here: the difference between
            // "flaky" and "broken", which no other number on this screen says.
            theme::right(theme::count_or_dim(with_error, Emphasis::Warn)),
        ]);
    }
    theme::block(&table)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render;

    fn fixture() -> Envelope<Value> {
        let bytes = include_bytes!("../../fixtures/status.json");
        serde_json::from_slice(bytes).expect("the status fixture is a valid envelope")
    }

    fn plain(styled: &str) -> String {
        anstream::adapter::strip_str(styled).to_string()
    }

    #[test]
    fn roots_and_queue_are_both_on_screen() {
        let screen = plain(&render::render(&fixture(), &RenderOptions::default()));
        assert!(screen.contains("flowspace3"), "{screen}");
        assert!(screen.contains("/Users/jordan/notes/inbox"), "{screen}");
        assert!(screen.contains("summarize"), "{screen}");
        assert!(screen.contains("embed"), "{screen}");
    }

    #[test]
    fn the_queue_is_pivoted_to_one_row_per_kind() {
        let screen = plain(&render::render(&fixture(), &RenderOptions::default()));
        // The fixture carries four `summarize` buckets; the table must show one.
        // Count TABLE rows only: `summarize` also appears in the last-error
        // line, which is a different fact about the same job kind.
        let rows = screen
            .lines()
            .filter(|line| line.trim_start().starts_with("│ summarize"))
            .count();
        assert_eq!(rows, 1, "expected one summarize row:\n{screen}");
    }

    #[test]
    fn the_header_counts_outstanding_work_not_total_work() {
        let screen = plain(&render::render(&fixture(), &RenderOptions::default()));
        // 419 + 168 pending, 4 running, and 7 failed are NOT queued work.
        assert!(screen.contains("591 queued"), "{screen}");
        assert!(screen.contains("7 failed"), "{screen}");
    }

    #[test]
    fn a_database_ahead_of_the_binary_is_shouted_about() {
        let ahead: Envelope<Value> = serde_json::from_str(
            r#"{"ok":true,"command":"status","v":1,
                "data":{"roots":[],"queue":[],"schema_ahead":[8,9]}}"#,
        )
        .unwrap();
        let screen = plain(&render::render(&ahead, &RenderOptions::default()));
        assert!(screen.contains("AHEAD of this binary"), "{screen}");
        assert!(screen.contains("8, 9"), "{screen}");
    }

    #[test]
    fn no_roots_says_how_to_get_one() {
        let empty: Envelope<Value> = serde_json::from_str(
            r#"{"ok":true,"command":"status","v":1,"data":{"roots":[],"queue":[]}}"#,
        )
        .unwrap();
        let screen = plain(&render::render(&empty, &RenderOptions::default()));
        assert!(screen.contains("flowspace3 add"), "{screen}");
        assert!(screen.contains("idle"), "{screen}");
    }

    #[test]
    fn the_last_error_is_shown_with_the_job_that_produced_it() {
        let screen = plain(&render::render(&fixture(), &RenderOptions::default()));
        assert!(screen.contains("summarize:blob=9f21c0e4b1a7"), "{screen}");
        assert!(screen.contains("FS3-E-PROVIDER-RATE-LIMITED"), "{screen}");
    }
}
