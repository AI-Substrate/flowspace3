//! Best-effort scan progress for `add` and `scan`.
//!
//! The event subscription is polled alongside the POST, never before it. A
//! missing, slow, or malformed stream is silent; there are no retries. The
//! losing stream future is dropped and its visible terminal line erased before
//! the envelope is returned to the caller.

use std::future::Future;
use std::io::Write;

use crate::DaemonClient;
use fs3_core::envelope::Envelope;
use fs3_core::events::{Event, EventKind, Hello, STREAM_NAME, STREAM_VERSION};

/// Run an envelope-producing request while drawing any scan events that arrive.
///
/// The POST and subscription start in the same `select!`; stream setup cannot
/// add latency to the answer. Once the POST settles the stream is cancelled,
/// its line is erased, and only then is the envelope returned.
pub async fn while_pending(
    client: &DaemonClient,
    requested_root: &str,
    operation: impl Future<Output = Envelope>,
) -> Envelope {
    let progress = consume(client, requested_root);
    tokio::pin!(operation);
    tokio::pin!(progress);

    tokio::select! {
        envelope = &mut operation => envelope,
        () = &mut progress => operation.await,
    }
}

async fn consume(client: &DaemonClient, requested_root: &str) {
    let mut response = match client.events(None).await {
        Ok(response) => response,
        Err(_) => return,
    };

    let mut bytes = Vec::new();
    let mut greeted = false;
    let mut line = VisibleLine::default();
    loop {
        let chunk = match response.chunk().await {
            Ok(Some(chunk)) => chunk,
            _ => return,
        };
        bytes.extend_from_slice(&chunk);
        while let Some(end) = bytes.iter().position(|byte| *byte == b'\n') {
            let raw = bytes.drain(..=end).collect::<Vec<_>>();
            let raw = &raw[..raw.len().saturating_sub(1)];
            if raw.is_empty() {
                continue;
            }
            if !greeted {
                let Ok(hello) = serde_json::from_slice::<Hello>(raw) else {
                    return;
                };
                if hello.stream != STREAM_NAME || hello.v != STREAM_VERSION {
                    return;
                }
                greeted = true;
                continue;
            }
            let Ok(event) = serde_json::from_slice::<Event>(raw) else {
                continue;
            };
            if let Some(text) = format_event(&event, requested_root) {
                line.draw(&text);
            }
        }
    }
}

fn format_event(event: &Event, requested_root: &str) -> Option<String> {
    let EventKind::ScanProgress {
        root,
        root_path,
        files_seen,
        enqueued,
        current,
    } = &event.kind
    else {
        return None;
    };
    if root_path != requested_root {
        return None;
    }
    let place = current.as_deref().unwrap_or(root_path);
    Some(format!(
        "scanning {root}  {files_seen} files · {enqueued} queued  {place}"
    ))
}

#[derive(Default)]
struct VisibleLine {
    visible: bool,
}

impl VisibleLine {
    fn draw(&mut self, text: &str) {
        let mut stderr = std::io::stderr().lock();
        let _ = write!(stderr, "\r\x1b[2K  {text}");
        let _ = stderr.flush();
        self.visible = true;
    }
}

impl Drop for VisibleLine {
    fn drop(&mut self) {
        if self.visible {
            let mut stderr = std::io::stderr().lock();
            let _ = write!(stderr, "\r\x1b[2K");
            let _ = stderr.flush();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recorded_stream_supplies_honest_counts_and_current_path() {
        let fixture = include_bytes!("../../tests/fixtures/scan-progress.ndjson");
        let events: Vec<Event> = fixture
            .split(|byte| *byte == b'\n')
            .skip(1)
            .filter(|line| !line.is_empty())
            .map(|line| serde_json::from_slice(line).unwrap())
            .collect();
        assert_eq!(
            format_event(&events[0], "/srv/api").as_deref(),
            Some(
                "scanning git:github.com/AI-Substrate/flowspace3  1200 files · 900 queued  crates/cli/src/main.rs"
            )
        );
        assert_eq!(format_event(&events[1], "/srv/api"), None);
        assert!(
            format_event(&events[2], "/srv/api")
                .unwrap()
                .ends_with("/srv/api")
        );
    }

    #[test]
    fn unrelated_events_do_not_draw() {
        let event = Event::new("2026-08-28T03:11:20.000Z", EventKind::Heartbeat { seq: 1 });
        assert_eq!(format_event(&event, "/srv/api"), None);
    }
}
