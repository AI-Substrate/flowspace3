//! Live daemon events for `flowspace3 status --watch`.
//!
//! This is intentionally not an envelope path. JSON/piped mode copies the
//! daemon's NDJSON bytes to stdout unchanged; human mode turns each complete
//! line into one terse sentence. A screen belongs to the TUI, not here.

use std::io::Write;

use crate::DaemonClient;
use anyhow::{Context, Result};
use fs3_core::events::{Event, EventKind, Hello};

/// Connect to `/events` and copy it until the daemon or caller disconnects.
///
/// # Errors
/// The daemon is unreachable, refuses the request, or emits an invalid line.
pub async fn run(client: &DaemonClient, heartbeat_ms: Option<u64>, human: bool) -> Result<()> {
    let mut response = client
        .events(heartbeat_ms)
        .await
        .map_err(|failure| anyhow::anyhow!(failure.render()))?;

    if !human {
        let mut stdout = std::io::stdout().lock();
        while let Some(chunk) = response.chunk().await.context("reading the event stream")? {
            stdout
                .write_all(&chunk)
                .context("writing the event stream")?;
            stdout.flush().context("flushing the event stream")?;
        }
        return Ok(());
    }

    let mut pending = Vec::new();
    while let Some(chunk) = response.chunk().await.context("reading the event stream")? {
        pending.extend_from_slice(&chunk);
        while let Some(end) = pending.iter().position(|byte| *byte == b'\n') {
            let line: Vec<u8> = pending.drain(..=end).collect();
            render_line(&line[..line.len() - 1])?;
        }
    }
    if !pending.is_empty() {
        anyhow::bail!("the event stream ended with an unterminated line");
    }
    Ok(())
}

fn render_line(line: &[u8]) -> Result<()> {
    let value: serde_json::Value = serde_json::from_slice(line).context("invalid event JSON")?;
    if value.get("stream").is_some() {
        let hello: Hello = serde_json::from_value(value).context("invalid event hello")?;
        println!(
            "watching {} v{} (daemon {}, heartbeat {}ms)",
            hello.stream, hello.v, hello.daemon, hello.heartbeat_ms
        );
        return Ok(());
    }

    let event: Event = serde_json::from_value(value).context("invalid event line")?;
    match event.kind {
        EventKind::JobDone {
            job,
            subject,
            ms,
            left,
        } => println!("{} done {job} {subject} ({ms}ms, {left} left)", event.at),
        EventKind::JobFailed {
            job,
            subject,
            error,
            attempts,
            terminal,
        } => println!(
            "{} {} {job} {subject} after {attempts} attempt(s): {error}",
            event.at,
            if terminal { "failed" } else { "retrying" }
        ),
        EventKind::Queue { rows } => {
            let summary = rows
                .into_iter()
                .map(|row| format!("{}:{}={}", row.kind, row.state, row.count))
                .collect::<Vec<_>>()
                .join(" ");
            println!("{} queue {summary}", event.at);
        }
        EventKind::ScanProgress {
            root_path,
            files_seen,
            enqueued,
            current,
            ..
        } => println!(
            "{} scan {}: {} seen, {} queued{}",
            event.at,
            root_path,
            files_seen,
            enqueued,
            current.map_or_else(String::new, |path| format!(" ({path})"))
        ),
        EventKind::RootChanged {
            change,
            root_path,
            files,
            ..
        } => println!("{} root {change}: {root_path} ({files} files)", event.at),
        EventKind::Heartbeat { .. } | EventKind::Unknown => {}
    }
    Ok(())
}
