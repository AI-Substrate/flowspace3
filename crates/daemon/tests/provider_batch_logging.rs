//! Console output is provider-grouped: one line per embed call and one per
//! summary dispatch wave, with per-item settlement detail below INFO.

mod support;

use std::fs;
use std::sync::Arc;
use std::time::Duration;

use fs3_core::{Config, DaemonConfig, DatabaseConfig, Element, ElementKind, Span};
use fs3_daemon::enrich::{SUMMARIZE, SummarizeJob};
use fs3_daemon::runner;
use fs3_daemon::wiring::AppState;
use fs3_testkit::fakes::{FakeEmbedder, FakeSummarizer};
use serde_json::json;

const IDENTITY: &str = "git:github.com/fs3/provider-batch-logging";

#[tokio::test]
async fn info_logs_report_provider_groups_without_item_subjects() {
    let log_dir = tempfile::tempdir().expect("log directory");
    let daemon = DaemonConfig {
        log_dir: log_dir.path().display().to_string(),
        log_level: "fs3_daemon=info".to_string(),
        ..DaemonConfig::default()
    };
    let logging = fs3_daemon::logging::init(&daemon);
    let log_path = logging.file.clone().expect("active log file");

    let database = support::FreshDatabase::create("provider-batch-logging").await;
    let config = Config {
        daemon,
        database: DatabaseConfig {
            url: database.url(),
        },
        ..Config::default()
    };
    let mut state = AppState::from_config(config).expect("wires");
    fs3_store::migrate(&state.db).await.expect("migrates");
    state.embedder = Arc::new(FakeEmbedder {
        dimensions: fs3_store::EMBEDDING_DIMENSIONS,
        ..FakeEmbedder::default()
    });
    state.summarizer = Arc::new(FakeSummarizer::default());

    let raw_items = support::items(1_000..1_002);
    support::hold(&state, "raw-log-batch", &raw_items).await;
    fs3_store::enqueue_job(
        &state.db,
        "embed",
        "embed:log:raw",
        &json!({
            "identity": IDENTITY,
            "source": "raw",
            "items": raw_items,
        }),
        Duration::ZERO,
    )
    .await
    .expect("queues raw embed batch");

    let smart_inputs = support::items(2_000..2_016);
    support::hold(&state, "smart-log-batch", &smart_inputs).await;
    for (index, (raw_hash, text)) in smart_inputs.into_iter().enumerate() {
        let job = SummarizeJob {
            identity: IDENTITY.to_string(),
            raw_hash,
            element: Element::new(
                ElementKind::Function,
                "function_item",
                format!("f{index}"),
                format!("src/log.rs::f{index}"),
                Span::new(index as u32 + 1, index as u32 + 1),
                text,
            ),
        };
        fs3_store::enqueue_job(
            &state.db,
            SUMMARIZE,
            &job.dedupe_key(),
            &serde_json::to_value(job).expect("summary payload"),
            Duration::ZERO,
        )
        .await
        .expect("queues summary");
    }

    runner::drain(&state, 16).await;

    let written = fs::read_to_string(&log_path).expect("reads provider log");
    assert!(
        written.contains("summarize: dispatched group of 16 items"),
        "summary wave is one grouped INFO line: {written}"
    );
    assert!(
        written.contains("embed: sent batch of 2 texts") && written.contains("source=\"raw\""),
        "raw embed provider calls use the grouped shape: {written}"
    );
    assert!(
        written.contains("embed: sent batch of 16 texts") && written.contains("source=\"smart\""),
        "smart embed provider calls use the same grouped shape: {written}"
    );
    assert!(
        written.contains("outcome=\"ok\"") && written.contains(" ms="),
        "group lines carry outcome and duration: {written}"
    );
    assert!(
        !written.contains("src/log.rs::f") && !written.contains("subject="),
        "per-item subjects stay below INFO: {written}"
    );

    database.destroy(state.db.clone()).await;
}
