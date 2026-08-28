//! Lanes: each stage gets its own width, and no provider is exceeded.
//!
//! The point of a lane is that the stages are not alike. A summarize is one
//! chat call per element; an embed batch is one wide call carrying a token
//! budget of texts; a scan is local I/O. One shared `worker_concurrency` meant
//! the slowest of the three throttled the other two for no reason.
//!
//! The constraint that makes this correctness rather than tuning: a lane is
//! clamped PER IDENTITY by the provider's own `concurrency_ceiling`. The local
//! ONNX embedder's session sits behind a Mutex, so concurrency there is a lie
//! and it declares 1 — and a repo pointed at that box must not be given
//! Azure's width, nor Azure dropped to that box's because another repo uses it.

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

/// An embedder that records the HIGH-WATER MARK of concurrent calls.
///
/// Counting calls would prove nothing about a lane; the question is how many
/// were in flight at once.
#[derive(Debug, Default)]
struct ConcurrencyProbe {
    live: AtomicUsize,
    peak: AtomicUsize,
    ceiling: usize,
}

impl ConcurrencyProbe {
    fn with_ceiling(ceiling: usize) -> Self {
        Self {
            ceiling,
            ..Self::default()
        }
    }

    fn peak(&self) -> usize {
        self.peak.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl Embedder for ConcurrencyProbe {
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let live = self.live.fetch_add(1, Ordering::SeqCst) + 1;
        self.peak.fetch_max(live, Ordering::SeqCst);
        // Long enough that genuinely concurrent calls overlap and a serial
        // implementation cannot accidentally look parallel.
        tokio::time::sleep(Duration::from_millis(120)).await;
        self.live.fetch_sub(1, Ordering::SeqCst);
        Ok(texts
            .iter()
            .map(|_| vec![0.05f32; fs3_store::EMBEDDING_DIMENSIONS])
            .collect())
    }

    fn key(&self) -> String {
        format!("probe@{}", fs3_store::EMBEDDING_DIMENSIONS)
    }

    fn concurrency_ceiling(&self) -> usize {
        self.ceiling
    }

    /// This probe measures concurrency, not size: it accepts anything.
    fn max_input_tokens(&self) -> usize {
        usize::MAX
    }
}

async fn stack(
    label: &str,
    lane: usize,
    ceiling: usize,
) -> (support::FreshDatabase, AppState, Arc<ConcurrencyProbe>) {
    let database = support::FreshDatabase::create(label).await;
    let mut config = Config {
        database: DatabaseConfig {
            url: database.url(),
        },
        ..Config::default()
    };
    config.indexing.embed_lane = lane;

    let mut state = AppState::from_config(config).expect("wires");
    fs3_store::migrate(&state.db).await.expect("migrates");
    let probe = Arc::new(ConcurrencyProbe::with_ceiling(ceiling));
    state.embedder = probe.clone();
    (database, state, probe)
}

/// Enqueue `n` embed jobs, each big enough that the token budget cuts them
/// apart into separate provider calls — and register a root that HOLDS their
/// content, without which none of them reaches the provider at all.
///
/// The hold is a precondition of the measurement, not decoration: the embed
/// handler refuses to pay for content no registered root maps, so a lane fed
/// unheld hashes records a peak of ZERO concurrent calls and reads exactly
/// like a lane that does not work. It lives inside this helper rather than in
/// each test so the two cannot drift apart.
async fn enqueue_wide(state: &AppState, n: usize) {
    let items = support::items(0..u32::try_from(n).expect("a small lane"));
    support::hold(state, "lane", &items).await;

    // One item per job, but each job is its own batch because they are all
    // distinct repos — grouping is by identity, so this is the honest way to
    // produce parallel batches rather than one merged call.
    for (i, (hash, text)) in items.iter().enumerate() {
        fs3_store::enqueue_job(
            &state.db,
            "embed",
            &format!("embed:lane:{i}"),
            &json!({
                "identity": format!("git:repo{i}"),
                "source": "raw",
                "items": [[hash, text]],
            }),
            Duration::ZERO,
        )
        .await
        .expect("enqueues");
    }
}

/// Batches run CONCURRENTLY, up to the configured lane width.
///
/// Before lanes, `drain_embed` ran its batches in a for-loop and the peak was
/// always one — the merging bought a wide call and then made those wide calls
/// one at a time.
#[tokio::test]
async fn the_embed_lane_runs_batches_concurrently() {
    let (database, state, probe) = stack("lane_concurrent", 6, 64).await;
    enqueue_wide(&state, 6).await;

    runner::drain(&state, 2).await;

    assert!(
        probe.peak() > 1,
        "batches must overlap; a lane that runs them one at a time is not a lane"
    );
    assert!(
        probe.peak() <= 6,
        "and must not exceed the configured width, got {}",
        probe.peak()
    );

    let pool = state.db.clone();
    database.destroy(pool).await;
}

/// The provider's ceiling CLAMPS the lane, and it wins when it is lower.
///
/// A single-GPU box serving one model gains nothing from a second concurrent
/// request — it just queues while holding a connection — and the local ONNX
/// embedder's Mutex makes concurrency there actively false. The lane is our
/// number; the ceiling is the provider's, and the provider's is the one that
/// cannot be argued with.
#[tokio::test]
async fn a_providers_ceiling_beats_a_wider_lane() {
    let (database, state, probe) = stack("lane_ceiling", 8, 1).await;
    enqueue_wide(&state, 5).await;

    runner::drain(&state, 2).await;

    assert_eq!(
        probe.peak(),
        1,
        "a ceiling of 1 must hold whatever the lane says, got {}",
        probe.peak()
    );

    let pool = state.db.clone();
    database.destroy(pool).await;
}

/// Zero is refused by config validation rather than silently meaning "stop".
#[test]
fn a_lane_of_zero_is_a_config_error() {
    let mut config = Config::default();
    config.indexing.embed_lane = 0;
    let problems = config.validate().expect_err("zero is not a width");
    assert!(
        format!("{problems}").contains("embed_lane"),
        "and it must name the key: {problems}"
    );

    let mut config = Config::default();
    config.indexing.summarize_lane = 0;
    let problems = config.validate().expect_err("zero is not a width");
    assert!(format!("{problems}").contains("summarize_lane"));
}
