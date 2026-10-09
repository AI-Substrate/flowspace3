//! Searches never wait behind background embedding (brief 2026-10-09).
//!
//! Measured before this: with the embed lane saturating a rate-limited Azure
//! deployment, a search's query embed took 20-29 s and the CLI gave up near
//! 40 s. These tests pin the three fixes: a query goes ahead of rate-limited
//! background work, background holds while a query is in flight, a repeated
//! query never reaches the provider, and the background token budget really
//! paces embed jobs end to end.

mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use fs3_core::ports::Embedder;
use fs3_core::{Config, DatabaseConfig, Result};
use fs3_daemon::runner;
use fs3_daemon::wiring::AppState;
use serde_json::json;
use tokio::sync::Notify;
use tokio::time::Instant;

/// Counts calls, and can hold one named query until released.
#[derive(Debug, Default)]
struct Recorder {
    calls: AtomicUsize,
    hold_query: Option<(String, Arc<Notify>)>,
}

#[async_trait]
impl Embedder for Recorder {
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if let Some((query, release)) = &self.hold_query
            && texts.first() == Some(query)
        {
            release.notified().await;
        }
        Ok(texts
            .iter()
            .map(|_| vec![0.05f32; fs3_store::EMBEDDING_DIMENSIONS])
            .collect())
    }

    fn key(&self) -> String {
        format!("recorder@{}", fs3_store::EMBEDDING_DIMENSIONS)
    }

    fn concurrency_ceiling(&self) -> usize {
        usize::MAX
    }

    fn max_input_tokens(&self) -> usize {
        usize::MAX
    }
}

/// A state whose pool is never touched: query embedding needs no database.
fn offline(recorder: Arc<Recorder>, tokens_per_minute: u64) -> AppState {
    let mut config = Config {
        database: DatabaseConfig {
            url: "postgres://fs3@127.0.0.1:1/unused".to_string(),
        },
        ..Config::default()
    };
    config.indexing.embed_tokens_per_minute = tokens_per_minute;
    let mut state = AppState::from_config(config).expect("wires");
    state.embedder = recorder;
    state
}

/// After a 429 the provider asked for a minute's quiet. Background honours
/// it; a search does not have to.
#[tokio::test(start_paused = true)]
async fn a_query_goes_ahead_of_rate_limited_background_work() {
    let recorder = Arc::new(Recorder::default());
    let state = offline(recorder.clone(), 6_000_000);
    let governor = state.embed_governor_for("");
    governor.rate_limited(Some(Duration::from_secs(60)));

    let background = {
        let governor = governor.clone();
        tokio::spawn(async move { governor.admit_background(1_000).await })
    };

    let started = Instant::now();
    state
        .embed_query("", "where is the retry policy")
        .await
        .expect("the query embeds");
    assert!(
        started.elapsed() < Duration::from_millis(1),
        "the query must not inherit background's Retry-After: {:?}",
        started.elapsed()
    );
    assert_eq!(recorder.calls.load(Ordering::SeqCst), 1);
    assert!(!background.is_finished(), "background is still paused");

    tokio::time::timeout(Duration::from_secs(61), background)
        .await
        .expect("background resumes when the pause ends")
        .expect("task");
}

/// While a query embed is in flight, no new background request starts on the
/// same instance: the provider's capacity goes to the person waiting.
#[tokio::test(start_paused = true)]
async fn background_holds_while_a_query_is_in_flight() {
    let release = Arc::new(Notify::new());
    let recorder = Arc::new(Recorder {
        hold_query: Some(("slow query".to_string(), release.clone())),
        ..Recorder::default()
    });
    let state = offline(recorder, 0);

    let query = {
        let state = state.clone();
        tokio::spawn(async move { state.embed_query("", "slow query").await })
    };
    tokio::task::yield_now().await;

    let governor = state.embed_governor_for("");
    let background = tokio::spawn(async move { governor.admit_background(10).await });
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert!(
        !background.is_finished(),
        "background must wait for the query"
    );

    release.notify_waiters();
    query.await.expect("task").expect("the query embeds");
    tokio::time::timeout(Duration::from_secs(1), background)
        .await
        .expect("background goes once the query is done")
        .expect("task");
}

/// Paging, the prompt hook and an agent re-asking all repeat a query; the
/// vector comes from the cache, keyed by exact text and embedder.
#[tokio::test]
async fn a_repeated_query_is_answered_from_the_cache() {
    let recorder = Arc::new(Recorder::default());
    let state = offline(recorder.clone(), 0);

    let first = state
        .embed_query("", "retry backoff")
        .await
        .expect("embeds");
    let again = state
        .embed_query("", "retry backoff")
        .await
        .expect("embeds");
    assert_eq!(first, again);
    assert_eq!(
        recorder.calls.load(Ordering::SeqCst),
        1,
        "the second is a cache hit"
    );

    state
        .embed_query("", "Retry backoff")
        .await
        .expect("embeds");
    assert_eq!(
        recorder.calls.load(Ordering::SeqCst),
        2,
        "a different text is not a hit"
    );
}

/// Real embed jobs through the runner: with a small token budget the drain is
/// paced; with pacing off the same work is not.
#[tokio::test]
async fn the_token_budget_paces_real_embed_jobs() {
    async fn drain_seconds(label: &str, tokens_per_minute: u64) -> f64 {
        let database = support::FreshDatabase::create(label).await;
        let mut config = Config {
            database: DatabaseConfig {
                url: database.url(),
            },
            ..Config::default()
        };
        // 10k tokens/s with a 100k burst.
        config.indexing.embed_tokens_per_minute = tokens_per_minute;
        let mut state = AppState::from_config(config).expect("wires");
        fs3_store::migrate(&state.db).await.expect("migrates");
        state.embedder = Arc::new(Recorder::default());

        // Three jobs of ~60k estimated tokens each, distinct identities so
        // they are three provider calls. A request is admitted while the
        // bucket is positive and its cost paid afterwards, so the first two
        // (120k) overdraw the 100k burst by ~20k and the third waits ~2 s.
        let items: Vec<(String, String)> = (0..3)
            .map(|n| {
                // ~180KB, so ~60k estimated tokens (bytes / 3).
                let text = format!("fn f{n}() {{}}\n").repeat(16_400);
                (fs3_core::content_hash(text.as_bytes()), text)
            })
            .collect();
        support::hold(&state, label, &items).await;
        for (i, (hash, text)) in items.iter().enumerate() {
            fs3_store::enqueue_job(
                &state.db,
                "embed",
                &format!("embed:paced:{i}"),
                &json!({
                    "identity": format!("git:paced{i}"),
                    "source": "raw",
                    "items": [[hash, text]],
                }),
                Duration::ZERO,
            )
            .await
            .expect("enqueues");
        }

        let started = std::time::Instant::now();
        runner::drain(&state, 2).await;
        let seconds = started.elapsed().as_secs_f64();
        let pool = state.db.clone();
        database.destroy(pool).await;
        seconds
    }

    let paced = drain_seconds("embed_paced", 600_000).await;
    let unpaced = drain_seconds("embed_unpaced", 0).await;
    assert!(
        paced >= 1.5,
        "a 100k burst cannot cover 180k without waiting: {paced:.2}s"
    );
    assert!(
        paced > unpaced + 1.0,
        "the budget, not the work, is what took the time: paced {paced:.2}s vs unpaced {unpaced:.2}s"
    );
}
