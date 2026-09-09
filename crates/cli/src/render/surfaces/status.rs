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
            theme::header("hidden"),
            theme::header("files"),
        ]);
        for root in &report.roots {
            roots.add_row([
                Cell::new(format!("{}", root.identity.bright_white())),
                Cell::new(format!("{}", root.root_path.bright_black())),
                Cell::new(if root.include_hidden { "yes" } else { "no" }),
                theme::right(theme::count(root.files)),
            ]);
        }
        out.push_str(&theme::block(&roots));
    }

    if let Some(retention) = &report.retention {
        let last_purge = retention.last_purge_at.as_deref().unwrap_or("not run");
        out.push_str(&format!(
            "\n{}{} {}d · last {} · purged {}\n",
            theme::GUTTER,
            "retention".bright_black(),
            retention.window_days,
            last_purge,
            retention.purged_last_run,
        ));
    }

    if let Some(conversations) = &report.conversations {
        out.push_str(&format!(
            "\n{}conversations {} · last poll {}\n",
            theme::GUTTER,
            conversations.state.as_str(),
            conversations.last_poll_at.as_deref().unwrap_or("pending"),
        ));
        if let Some(reason) = &conversations.state_reason {
            out.push_str(&format!("{}  {reason}\n", theme::GUTTER));
        }
        for harness in &conversations.harnesses {
            out.push_str(&format!(
                "{}  {}: {} tracked · {} behind · {} unreadable · newest ingest {}\n",
                theme::GUTTER,
                harness.harness,
                harness.tracked,
                harness.behind,
                harness.unreadable,
                harness.newest_ingest.as_ref().map_or_else(
                    || "unavailable (not observed since daemon boot)".to_owned(),
                    |receipt| receipt.describe(),
                ),
            ));
        }
    }

    if !report.queue.is_empty() {
        let mut grouped: BTreeMap<&str, BTreeMap<&str, &QueueRow>> = BTreeMap::new();
        for row in &report.queue {
            grouped
                .entry(&row.kind)
                .or_default()
                .insert(&row.state, row);
        }
        let history = report.queue.iter().any(|row| row.state == "done");
        let mut queue = theme::table(width);
        if history {
            queue.set_header([
                theme::header("job"),
                theme::header("progress"),
                theme::header("done"),
                theme::header("running"),
                theme::header("pending"),
                theme::header("failed"),
            ]);
        } else {
            queue.set_header([
                theme::header("job"),
                theme::header("running"),
                theme::header("pending"),
                theme::header("failed"),
            ]);
        }
        for (kind, states) in grouped {
            let count = |state: &str| states.get(state).map_or(0, |row| row.count);
            let done = count("done");
            let running = count("running");
            let pending = count("pending");
            let failed = count("failed");
            if history {
                queue.add_row([
                    Cell::new(format!("{}", kind.bright_white())),
                    Cell::new(theme::meter(done, done + running + pending + failed, 12)),
                    theme::right(theme::count(done)),
                    theme::right(theme::count(running)),
                    theme::right(theme::count(pending)),
                    theme::right(theme::count(failed)),
                ]);
            } else {
                queue.add_row([
                    Cell::new(format!("{}", kind.bright_white())),
                    theme::right(theme::count(running)),
                    theme::right(theme::count(pending)),
                    theme::right(theme::count(failed)),
                ]);
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn plain(screen: &str) -> String {
        anstream::adapter::strip_str(screen).to_string()
    }

    fn envelope(queue: Value) -> Envelope<Value> {
        serde_json::from_value(json!({
            "ok": true,
            "command": "status",
            "v": 1,
            "data": {
                "roots": [],
                "queue": queue,
                "retention": {
                    "window_days": 1,
                    "last_purge_at": "2026-09-02T01:00:00.000Z",
                    "purged_last_run": 3
                },
                "last_error": null,
                "inconsistencies": [],
                "schema_ahead": []
            }
        }))
        .unwrap()
    }

    #[test]
    fn live_status_has_no_fake_done_progress_and_shows_retention() {
        let screen = plain(
            &render(
                &envelope(json!([
                    {"kind": "embed", "state": "pending", "count": 2, "with_error": 0}
                ])),
                100,
            )
            .unwrap(),
        );
        assert!(screen.contains("retention 1d"), "{screen}");
        assert!(screen.contains("purged 3"), "{screen}");
        assert!(!screen.contains("progress"), "{screen}");
        assert!(!screen.contains("done"), "{screen}");
    }

    #[test]
    fn roots_show_their_hidden_directory_policy() {
        let value: Envelope<Value> = serde_json::from_value(json!({
            "ok": true,
            "command": "status",
            "v": 1,
            "data": {
                "roots": [{
                    "identity": "git:example/repo",
                    "root_path": "/srv/repo",
                    "include_hidden": true,
                    "files": 2
                }],
                "queue": [],
                "retention": null,
                "last_error": null,
                "inconsistencies": [],
                "schema_ahead": []
            }
        }))
        .unwrap();
        let screen = plain(&render(&value, 100).unwrap());
        assert!(screen.contains("hidden"), "{screen}");
        assert!(screen.contains("yes"), "{screen}");
    }

    #[test]
    fn explicit_history_restores_done_progress() {
        let screen = plain(
            &render(
                &envelope(json!([
                    {"kind": "embed", "state": "done", "count": 7, "with_error": 1}
                ])),
                100,
            )
            .unwrap(),
        );
        assert!(screen.contains("progress"), "{screen}");
        assert!(screen.contains("done"), "{screen}");
        assert!(screen.contains('7'), "{screen}");
    }

    #[test]
    fn conversations_render_daemon_state_without_guessing_from_counts() {
        let home = tempfile::tempdir().unwrap();
        assert!(home.path().is_dir());
        for state in ["flowing", "stalled", "disabled"] {
            let mut value = envelope(json!([]));
            value.data.as_mut().unwrap()["conversations"] = json!({
                "state": state, "state_reason": "daemon-authored explanation", "last_poll_at": null,
                "harnesses": [{"harness":"omp","tracked":3,"behind":2,"unreadable":1,"newest_ingest_at":"2026-09-01T00:00:00Z"}]
            });
            let screen = plain(&render(&value, 100).unwrap());
            assert!(
                screen.contains(&format!("conversations {state}")),
                "{screen}"
            );
            assert!(screen.contains("daemon-authored explanation"));
            assert!(screen.contains("3 tracked · 2 behind · 1 unreadable"));
            assert!(screen.contains("last poll pending"));
            assert!(screen.contains("unavailable (not observed since daemon boot)"));
        }
    }

    #[test]
    fn newest_ingest_renders_actual_rescan_counters() {
        let mut value = envelope(json!([]));
        value.data.as_mut().unwrap()["conversations"] = json!({
            "state":"flowing","last_poll_at":null,
            "harnesses":[{"harness":"claude","tracked":1,"behind":0,"newest_ingest_at":"2026-09-06T00:00:00Z",
                "newest_ingest":{"at":"2026-09-06T00:00:00Z","address":"conv:proof","records_read":2,"turns_new":0,"deduped":2,"summarized":0,"rescanned":true,"contended":0}}]
        });
        let screen = plain(&render(&value, 100).unwrap());
        for text in [
            "conv:proof",
            "read 2",
            "new 0",
            "deduped 2",
            "summarized 0",
            "rescanned true",
        ] {
            assert!(screen.contains(text), "{screen}");
        }
    }
}
