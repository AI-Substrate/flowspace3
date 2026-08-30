//! End-to-end proof for chunked conversation embeddings.

mod support;

use std::sync::{Arc, Mutex};

use fs3_core::{Config, DatabaseConfig, Turn, TurnRole, TurnSource};
use fs3_daemon::wiring::AppState;
use fs3_testkit::fakes::FakeEmbedder;
use tracing::subscriber::DefaultGuard;
use tracing_subscriber::fmt::MakeWriter;

const GUID: &str = "9fca61cc-2c7a-4d9d-93eb-1dc92ba69b55";
const ANCHOR: &str = "tail_anchor_only_beyond_the_old_prefix";
const PROVIDER_CAP: usize = 8_192;

#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Captured {
    fn install(&self) -> DefaultGuard {
        let subscriber = tracing_subscriber::fmt()
            .with_writer(self.clone())
            .with_max_level(tracing::Level::INFO)
            .with_ansi(false)
            .finish();
        tracing::subscriber::set_default(subscriber)
    }

    fn text(&self) -> String {
        String::from_utf8(self.0.lock().expect("the log is not poisoned").clone())
            .expect("log output is utf-8")
    }
}

impl std::io::Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().expect("the log is not poisoned").extend(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for Captured {
    type Writer = Self;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

struct LiveDaemon {
    base: String,
    task: tokio::task::JoinHandle<()>,
}

impl LiveDaemon {
    async fn spawn(state: AppState, auth: fs3_daemon::Auth) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("an ephemeral port should be available");
        let address = listener.local_addr().expect("the socket is bound");
        let task = tokio::spawn(async move {
            axum::serve(listener, fs3_daemon::router(state, auth))
                .await
                .expect("the isolated daemon serves");
        });
        Self {
            base: format!("http://{address}"),
            task,
        }
    }
}

impl Drop for LiveDaemon {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// [`FreshDatabase`](fs3_testkit::FreshDatabase) deliberately uses explicit
/// async cleanup. This guard adds a failure-path cleanup for this live fixture:
/// unwinding moves cleanup to a fresh runtime on another thread, where blocking
/// cannot deadlock the test runtime.
struct DatabaseGuard(Option<support::FreshDatabase>);

impl DatabaseGuard {
    async fn destroy(mut self, pool: fs3_store::PgPool) {
        self.0
            .take()
            .expect("database guard is armed")
            .destroy(pool)
            .await;
    }
}

impl Drop for DatabaseGuard {
    fn drop(&mut self) {
        let Some(database) = self.0.take() else {
            return;
        };
        std::thread::spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("cleanup runtime")
                .block_on(database.destroy_force());
        })
        .join()
        .expect("database cleanup thread");
    }
}

fn turn(turn_no: u32, role: TurnRole, body: String) -> Turn {
    Turn {
        turn_no,
        role,
        source: TurnSource::Peer,
        head_sha: None,
        at: "2026-08-30T05:00:00Z".to_string(),
        body,
        items: Vec::new(),
    }
}

fn whale() -> String {
    let prefix = "ordinary prefix material with no retrieval marker ".repeat(500);
    let tail = format!("{ANCHOR} ").repeat(250);
    let body = format!("{prefix}{tail}");
    assert!(
        (24_000..=30_000).contains(&body.len()),
        "fixture must remain approximately 25KB, got {} bytes",
        body.len()
    );
    assert!(
        fs3_core::estimate_tokens(&body) > PROVIDER_CAP,
        "fixture must cross the provider cap"
    );
    body
}

/// A real HTTP router, disposable database and capped recording provider prove
/// the product path: intake, queue drain, tail search, provider hygiene and log
/// hygiene. Binary config discovery is intentionally outside this fixture.
#[tokio::test]
async fn an_isolated_daemon_searches_a_whale_tail_without_sending_empty_input() {
    let database = support::FreshDatabase::create("embedsplitlive").await;
    let database = DatabaseGuard(Some(database));
    let config = Config {
        database: DatabaseConfig {
            url: database.0.as_ref().expect("database guard is armed").url(),
        },
        ..Config::default()
    };
    let mut state = AppState::from_config(config).expect("the fake stack wires");
    fs3_store::migrate(&state.db).await.expect("migrates");

    let embedder = Arc::new(FakeEmbedder {
        dimensions: fs3_store::EMBEDDING_DIMENSIONS,
        ..FakeEmbedder::capped(PROVIDER_CAP)
    });
    state.embedder = embedder.clone();

    let auth = support::auth("embed-split-live");
    let daemon = LiveDaemon::spawn(state.clone(), auth.auth).await;
    let client = reqwest::Client::new();
    let log = Captured::default();

    {
        let _guard = log.install();
        let response = client
            .post(format!("{}/conversations", daemon.base))
            .bearer_auth(&auth.key)
            .json(&serde_json::json!({
                "guid": GUID,
                "repo_identity": "git:github.com/fs3/embed-split-fixture",
                "worktree": "/srv/embed-split-fixture",
                "title": "chunked whale fixture",
                "started_at": "2026-08-30T05:00:00Z",
                "turns": [
                    turn(1, TurnRole::Human, whale()),
                    turn(2, TurnRole::Agent, String::new())
                ]
            }))
            .send()
            .await
            .expect("conversation intake answers");
        assert!(
            response.status().is_success(),
            "intake failed: {response:?}"
        );

        let drained = fs3_daemon::drain(&state, 1).await;
        assert_eq!(drained.failed, 0, "all enrichment must settle: {drained:?}");
    }

    let received_before_search = embedder.received();
    assert!(
        !received_before_search.is_empty(),
        "the whale must reach the provider in chunks"
    );
    assert!(
        received_before_search
            .iter()
            .all(|text| !text.trim().is_empty()),
        "the empty turn reached the provider: {received_before_search:?}"
    );
    assert!(
        received_before_search
            .iter()
            .all(|text| fs3_core::estimate_tokens(text) <= PROVIDER_CAP),
        "every provider input must fit its declared cap"
    );
    assert!(
        received_before_search
            .iter()
            .any(|text| text.contains(ANCHOR)),
        "the whale tail never reached the provider"
    );

    let response = client
        .get(format!("{}/search", daemon.base))
        .bearer_auth(&auth.key)
        .query(&[
            ("q", ANCHOR),
            ("repo", "all"),
            ("source", "conversation"),
            ("min_score", "0.90"),
            ("limit", "5"),
        ])
        .send()
        .await
        .expect("tail search answers");
    assert!(
        response.status().is_success(),
        "search failed: {response:?}"
    );
    let envelope: serde_json::Value = response.json().await.expect("search envelope is JSON");
    let results = envelope["data"]["results"]
        .as_array()
        .expect("search result array");
    assert!(
        results
            .iter()
            .any(|hit| hit["address"] == format!("conv:{GUID}#t1")),
        "tail query did not resolve to the whale turn: {envelope:#}"
    );

    let log = log.text();
    assert!(
        !log.contains("input cannot be an empty string"),
        "empty-input provider rejection reached the log: {log}"
    );
    assert!(
        !log.contains("input exceeds the model's per-input cap; embedding a prefix of it"),
        "legacy per-input-cap warning reached the log: {log}"
    );

    drop(daemon);
    database.destroy(state.db).await;
}
