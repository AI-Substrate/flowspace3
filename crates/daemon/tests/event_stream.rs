//! The daemon event stream from producer seams to concurrent consumers.

mod support;

use std::time::Duration;

use fs3_core::events::{Event, EventKind};
use fs3_core::{Config, DatabaseConfig};
use fs3_daemon::runner;
use fs3_daemon::wiring::AppState;
use serde_json::json;
use tokio::sync::broadcast::error::TryRecvError;

async fn stack(label: &str) -> (support::FreshDatabase, AppState) {
    let database = support::FreshDatabase::create(label).await;
    let config = Config {
        database: DatabaseConfig { url: database.url() },
        ..Config::default()
    };
    let state = AppState::from_config(config).expect("the fake stack wires");
    fs3_store::migrate(&state.db).await.expect("migrates");
    (database, state)
}

fn drain_events(receiver: &mut tokio::sync::broadcast::Receiver<Event>) -> Vec<Event> {
    let mut events = Vec::new();
    loop {
        match receiver.try_recv() {
            Ok(event) => events.push(event),
            Err(TryRecvError::Empty | TryRecvError::Closed) => return events,
            Err(TryRecvError::Lagged(skipped)) => panic!("test watcher lagged by {skipped}"),
        }
    }
}

#[tokio::test]
async fn fanout_is_identical_and_a_lagging_watcher_is_dropped() {
    let state = AppState::from_config(Config::default()).expect("the fake stack wires");
    let mut first = state.subscribe();
    let mut second = state.subscribe();
    state.emit(EventKind::RootChanged {
        change: "added".to_string(),
        root: "path:/tmp/root".to_string(),
        root_path: "/tmp/root".to_string(),
        files: 7,
    });

    let first_event = first.recv().await.expect("first watcher receives");
    let second_event = second.recv().await.expect("second watcher receives");
    assert_eq!(first_event, second_event, "one event is fanned out, not rebuilt");
    assert_eq!(first_event.at.len(), 24, "RFC 3339 with milliseconds");
    assert!(first_event.at.ends_with('Z'));

    let mut slow = state.subscribe();
    for seq in 0..=AppState::event_capacity() {
        state.emit(EventKind::Heartbeat { seq: seq as u64 });
    }
    assert!(
        matches!(slow.try_recv(), Err(TryRecvError::Lagged(_))),
        "a full watcher buffer disconnects that watcher instead of blocking producers"
    );
}

#[tokio::test]
async fn serial_success_and_failure_emit_their_settlement_and_queue_snapshot() {
    let (database, state) = stack("eventsettle").await;
    let mut events = state.subscribe();

    fs3_store::enqueue_job(
        &state.db,
        "embed",
        "embed:event:ok",
        &json!({
            "identity": "git:test",
            "source": "raw",
            "items": [["0000000000000000000000000000000000000000", "body"]],
        }),
        Duration::ZERO,
    )
    .await
    .expect("enqueues embed");
    runner::drain(&state, 1).await;

    fs3_store::enqueue_job(
        &state.db,
        "scan_file",
        "scan:event:bad",
        &json!({ "not": "a scan payload" }),
        Duration::ZERO,
    )
    .await
    .expect("enqueues malformed scan");
    runner::drain(&state, 1).await;

    let events = drain_events(&mut events);
    assert!(events.iter().any(|event| matches!(
        &event.kind,
        EventKind::JobDone { job, .. } if job == "embed"
    )));
    assert!(events.iter().any(|event| matches!(
        &event.kind,
        EventKind::JobFailed { job, terminal: true, .. } if job == "scan_file"
    )));
    assert!(events.iter().any(|event| matches!(&event.kind, EventKind::Queue { .. })));

    let pool = state.db.clone();
    database.destroy(pool).await;
}

#[tokio::test]
async fn a_large_real_add_emits_bounded_progress_and_root_changes() {
    let (database, state) = stack("eventscan").await;
    let root = support::temp_dir("event-scan");
    std::fs::create_dir_all(root.join("src")).expect("creates source directory");
    for n in 0..1_025 {
        std::fs::write(root.join("src").join(format!("f{n}.rs")), format!("fn f{n}() {{}}\n"))
            .expect("writes source file");
    }

    let mut events = state.subscribe();
    let report = fs3_daemon::roots::add_root(&state, &root)
        .await
        .expect("adds the large root");
    assert_eq!(report.files, 1_025);

    let emitted = drain_events(&mut events);
    let progress: Vec<_> = emitted
        .iter()
        .filter_map(|event| match &event.kind {
            EventKind::ScanProgress {
                files_seen,
                enqueued,
                current,
                ..
            } => Some((*files_seen, *enqueued, current.clone())),
            _ => None,
        })
        .collect();
    assert!(progress.len() >= 5, "four 256-file pulses plus final totals: {progress:?}");
    assert!(progress.len() < 32, "the cadence must stay bounded: {} lines", progress.len());
    assert_eq!(progress.last(), Some(&(1_025, 1_025, None)));
    assert!(emitted.iter().any(|event| matches!(
        &event.kind,
        EventKind::RootChanged { change, files: 1_025, .. } if change == "added"
    )));

    let canonical = std::fs::canonicalize(&root).expect("canonical root");
    fs3_daemon::remove::remove(&state, &canonical.to_string_lossy())
        .await
        .expect("removes the root");
    let removed = drain_events(&mut events);
    assert!(removed.iter().any(|event| matches!(
        &event.kind,
        EventKind::RootChanged { change, files: 0, .. } if change == "removed"
    )));

    std::fs::remove_dir_all(root).ok();
    let pool = state.db.clone();
    database.destroy(pool).await;
}
