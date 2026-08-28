//! The daemon event stream from producer seams to concurrent consumers.

mod support;

use std::sync::Arc;
use std::time::Duration;

use fs3_core::events::{Event, EventKind};
use fs3_core::{Config, DatabaseConfig};
use fs3_daemon::runner;
use fs3_daemon::wiring::AppState;
use fs3_testkit::FakeEmbedder;
use serde_json::json;

struct RateLimited;

#[async_trait::async_trait]
impl fs3_core::Embedder for RateLimited {
    async fn embed(&self, _texts: &[String]) -> fs3_core::Result<Vec<Vec<f32>>> {
        Err(fs3_core::Error::RateLimited {
            provider: "test".to_string(),
            retry_after: Some(Duration::from_secs(60)),
            attempts: 1,
        })
    }

    fn key(&self) -> String {
        "rate-limited@test".to_string()
    }

    fn concurrency_ceiling(&self) -> usize {
        1
    }

    fn max_input_tokens(&self) -> usize {
        usize::MAX
    }
}
use tokio::sync::broadcast::error::TryRecvError;

async fn next_line(response: &mut reqwest::Response, pending: &mut Vec<u8>) -> Vec<u8> {
    loop {
        if let Some(end) = pending.iter().position(|byte| *byte == b'\n') {
            return pending.drain(..=end).collect();
        }
        let chunk = response
            .chunk()
            .await
            .expect("reads event response")
            .expect("event stream stays open");
        pending.extend_from_slice(&chunk);
    }
}

async fn open_events(base: &str, heartbeat_ms: Option<u64>) -> reqwest::Response {
    let mut request = reqwest::Client::new()
        .get(format!("{base}/events"))
        .bearer_auth("event-test-key");
    if let Some(heartbeat_ms) = heartbeat_ms {
        request = request.query(&[("heartbeat_ms", heartbeat_ms)]);
    }
    request.send().await.expect("opens event stream")
}

async fn stack(label: &str) -> (support::FreshDatabase, AppState) {
    let database = support::FreshDatabase::create(label).await;
    let config = Config {
        database: DatabaseConfig {
            url: database.url(),
        },
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
    assert_eq!(
        first_event, second_event,
        "one event is fanned out, not rebuilt"
    );
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
async fn two_http_watchers_receive_the_same_live_event_after_hello() {
    let state = AppState::from_config(Config::default()).expect("the fake stack wires");
    let auth = fs3_daemon::auth::Auth::new(
        "event-test-key",
        std::path::PathBuf::from("/tmp/fs3-event-test-key"),
    );
    let base = support::spawn(fs3_daemon::http::router(state.clone(), auth)).await;
    let (mut first, mut second) = tokio::join!(open_events(&base, None), open_events(&base, None));
    assert_eq!(
        first.headers()[reqwest::header::CONTENT_TYPE],
        "application/x-ndjson"
    );

    let mut first_pending = Vec::new();
    let mut second_pending = Vec::new();
    let first_hello = next_line(&mut first, &mut first_pending).await;
    let second_hello = next_line(&mut second, &mut second_pending).await;
    assert_eq!(first_hello, second_hello);
    serde_json::from_slice::<fs3_core::Hello>(&first_hello).expect("hello is first");

    state.emit(EventKind::RootChanged {
        change: "added".to_string(),
        root: "path:/tmp/two".to_string(),
        root_path: "/tmp/two".to_string(),
        files: 2,
    });
    let (first_event, second_event) = tokio::join!(
        tokio::time::timeout(
            Duration::from_secs(2),
            next_line(&mut first, &mut first_pending)
        ),
        tokio::time::timeout(
            Duration::from_secs(2),
            next_line(&mut second, &mut second_pending)
        )
    );
    let first_event = first_event.expect("first watcher receives promptly");
    let second_event = second_event.expect("second watcher receives promptly");
    assert_eq!(first_event, second_event, "fan-out bytes are identical");
    let event: Event = serde_json::from_slice(&first_event).expect("event line parses");
    assert!(matches!(event.kind, EventKind::RootChanged { .. }));
}

#[tokio::test]
async fn heartbeat_override_is_per_subscriber_and_keeps_the_stream_live() {
    let state = AppState::from_config(Config::default()).expect("the fake stack wires");
    let auth = fs3_daemon::auth::Auth::new(
        "event-test-key",
        std::path::PathBuf::from("/tmp/fs3-event-heartbeat-key"),
    );
    let base = support::spawn(fs3_daemon::http::router(state, auth)).await;
    let mut response = open_events(&base, Some(10)).await;
    let mut pending = Vec::new();
    let hello: fs3_core::Hello =
        serde_json::from_slice(&next_line(&mut response, &mut pending).await).expect("hello");
    assert_eq!(hello.heartbeat_ms, 10);
    let event: Event = serde_json::from_slice(
        &tokio::time::timeout(
            Duration::from_secs(1),
            next_line(&mut response, &mut pending),
        )
        .await
        .expect("heartbeat arrives"),
    )
    .expect("heartbeat parses");
    assert!(matches!(event.kind, EventKind::Heartbeat { seq: 1 }));
}

#[tokio::test]
async fn serial_success_and_failure_emit_their_settlement_and_queue_snapshot() {
    let (database, state) = stack("eventsettle").await;
    let mut events = state.subscribe();
    let root = support::temp_dir("event-serial");
    std::fs::write(root.join("one.rs"), "fn one() {}\n").expect("writes source file");
    fs3_daemon::roots::add_root(&state, &root)
        .await
        .expect("adds one serial scan");
    runner::drain(&state, 1).await;

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
        EventKind::JobDone { job, .. } if job == "scan_file"
    )));
    assert!(events.iter().any(|event| matches!(
        &event.kind,
        EventKind::JobFailed { job, terminal: true, .. } if job == "scan_file"
    )));
    assert!(
        events
            .iter()
            .any(|event| matches!(&event.kind, EventKind::Queue { .. }))
    );
    std::fs::remove_dir_all(root).ok();

    let pool = state.db.clone();
    database.destroy(pool).await;
}

#[tokio::test]
async fn a_failed_embed_batch_emits_a_nonterminal_retry() {
    let (database, mut state) = stack("eventretry").await;
    state.embedder = Arc::new(FakeEmbedder {
        dimensions: fs3_store::EMBEDDING_DIMENSIONS,
        ..FakeEmbedder::failing_after(0)
    });
    let items = support::items(0..1);
    support::hold(&state, "event-retry", &items).await;
    let (hash, text) = &items[0];
    fs3_store::enqueue_job(
        &state.db,
        "embed",
        "embed:event:retry",
        &json!({
            "identity": "git:test",
            "source": "raw",
            "items": [[hash, text]],
        }),
        Duration::ZERO,
    )
    .await
    .expect("enqueues embed");

    let mut events = state.subscribe();
    runner::drain(&state, 1).await;
    let events = drain_events(&mut events);
    assert!(events.iter().any(|event| matches!(
        &event.kind,
        EventKind::JobFailed { job, terminal: false, .. } if job == "embed"
    )));

    let pool = state.db.clone();
    database.destroy(pool).await;
}

#[tokio::test]
async fn a_rate_limited_embed_emits_a_nonterminal_park() {
    let (database, mut state) = stack("eventpark").await;
    state.embedder = Arc::new(RateLimited);
    let items = support::items(1..2);
    support::hold(&state, "event-park", &items).await;
    let (hash, text) = &items[0];
    fs3_store::enqueue_job(
        &state.db,
        "embed",
        "embed:event:park",
        &json!({
            "identity": "git:test",
            "source": "raw",
            "items": [[hash, text]],
        }),
        Duration::ZERO,
    )
    .await
    .expect("enqueues embed");

    let mut events = state.subscribe();
    let drained = runner::drain(&state, 1).await;
    assert_eq!(drained.parked, 1);
    let events = drain_events(&mut events);
    assert!(events.iter().any(|event| matches!(
        &event.kind,
        EventKind::JobFailed { job, terminal: false, .. } if job == "embed"
    )));

    let pool = state.db.clone();
    database.destroy(pool).await;
}
#[tokio::test]
async fn a_large_real_add_emits_bounded_progress_and_root_changes() {
    let (database, state) = stack("eventscan").await;
    let root = support::temp_dir("event-scan");
    std::fs::create_dir_all(root.join("src")).expect("creates source directory");
    for n in 0..1_025 {
        std::fs::write(
            root.join("src").join(format!("f{n}.rs")),
            format!("fn f{n}() {{}}\n"),
        )
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
    assert!(
        progress.len() >= 5,
        "four 256-file pulses plus final totals: {progress:?}"
    );
    assert!(
        progress.len() < 32,
        "the cadence must stay bounded: {} lines",
        progress.len()
    );
    assert_eq!(progress.last(), Some(&(1_025, 1_025, None)));
    assert!(emitted.iter().any(|event| matches!(
        &event.kind,
        EventKind::RootChanged { change, files: 1_025, .. } if change == "added"
    )));

    fs3_daemon::roots::rescan_root(&state, &root)
        .await
        .expect("rescans the root");
    let rescanned = drain_events(&mut events);
    assert!(rescanned.iter().any(|event| matches!(
        &event.kind,
        EventKind::RootChanged { change, files: 1_025, .. } if change == "rescanned"
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
