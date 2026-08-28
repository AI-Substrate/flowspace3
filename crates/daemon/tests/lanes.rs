//! Lanes: each stage gets its own width, and no provider is exceeded.
//!
//! The point of a lane is that the stages are not alike. An ingest is local
//! read/parse/store work; a summarize is one chat call per element; an embed
//! batch is one wide call carrying a token budget of texts; a scan is local I/O.
//! One shared `worker_concurrency` let provider backlog starve local work.
//!
//! The constraint that makes this correctness rather than tuning: a lane is
//! clamped PER IDENTITY by the provider's own `concurrency_ceiling`. The local
//! ONNX embedder's session sits behind a Mutex, so concurrency there is a lie
//! and it declares 1 — and a repo pointed at that box must not be given
//! Azure's width, nor Azure dropped to that box's because another repo uses it.

mod support;

use std::io::Write as _;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use fs3_core::ports::{Embedder, Summarizer};
use fs3_core::{Config, DatabaseConfig, Element, ElementKind, Result, Span, Summary};
use fs3_daemon::conversations::{ListRequest, list};
use fs3_daemon::convo_ingest::{IngestRequest, ingest, submit};
use fs3_daemon::enrich::{SUMMARIZE, SummarizeJob};
use fs3_daemon::roots::{self, ScanFileJob};
use fs3_daemon::runner;
use fs3_daemon::wiring::AppState;
use serde_json::json;
use tokio::sync::{Notify, Semaphore, watch};

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

/// A provider that holds enrichment in flight until the test releases it.
///
/// Calls are recorded before the wait, so observing one proves the general lane
/// is occupied rather than merely queued.
#[derive(Debug)]
struct BlockingSummarizer {
    calls: std::sync::Mutex<Vec<String>>,
    entered: Notify,
    permits: Semaphore,
}

impl Default for BlockingSummarizer {
    fn default() -> Self {
        Self {
            calls: std::sync::Mutex::new(Vec::new()),
            entered: Notify::new(),
            permits: Semaphore::new(0),
        }
    }
}

impl BlockingSummarizer {
    async fn wait_for_calls(&self, count: usize) {
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                if self.calls.lock().expect("calls lock").len() >= count {
                    return;
                }
                self.entered.notified().await;
            }
        })
        .await
        .expect("the summarize backlog starts");
    }

    fn release(&self, count: usize) {
        self.permits.add_permits(count);
    }

    fn calls(&self) -> Vec<String> {
        self.calls.lock().expect("calls lock").clone()
    }
}

#[async_trait]
impl Summarizer for BlockingSummarizer {
    async fn summarize(&self, element: &Element) -> Result<Summary> {
        self.calls
            .lock()
            .expect("calls lock")
            .push(element.address.clone());
        self.entered.notify_one();
        self.permits
            .acquire()
            .await
            .expect("gate remains open")
            .forget();
        Ok(Summary {
            text: format!("summary of {}", element.address),
            tags: vec!["lane".to_string()],
            ..Summary::default()
        })
    }

    fn key(&self) -> String {
        "blocking@1".to_string()
    }

    fn concurrency_ceiling(&self) -> usize {
        1
    }

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

/// General workers claim scan and summarize work concurrently. The scan is
/// held behind a database lock after claim; the provider call must still start,
/// proving the second worker was not merely configured but used.
#[tokio::test]
async fn general_workers_let_scans_and_summaries_progress_together() {
    let database = support::FreshDatabase::create("general-mixed-progress").await;
    let mut config = Config {
        database: DatabaseConfig {
            url: database.url(),
        },
        ..Config::default()
    };
    config.indexing.worker_concurrency = 2;
    let mut state = AppState::from_config(config).expect("wires");
    fs3_store::migrate(&state.db).await.expect("migrates");
    let blocker = Arc::new(BlockingSummarizer::default());
    state.summarizer = blocker.clone();

    let (raw_hash, text) = support::items(20_000..20_001).remove(0);
    support::hold(
        &state,
        "mixed-progress",
        &[(raw_hash.clone(), text.clone())],
    )
    .await;
    let summary = SummarizeJob {
        identity: "git:mixed-progress".to_string(),
        raw_hash,
        element: Element::new(
            ElementKind::Function,
            "function_item",
            "summarize_me",
            "src/summary.rs::summarize_me",
            Span::new(1, 12),
            text,
        ),
    };
    fs3_store::enqueue_job(
        &state.db,
        SUMMARIZE,
        &summary.dedupe_key(),
        &serde_json::to_value(&summary).expect("summary payload"),
        Duration::ZERO,
    )
    .await
    .expect("queues summary");

    let root = support::temp_dir("general-mixed-scan");
    roots::add_root(&state, &root)
        .await
        .expect("registers empty root");
    let file = root.join("fresh.rs");
    std::fs::write(&file, "pub fn freshly_scanned() {}\n").expect("writes scan fixture");
    let worktree = fs3_store::find_worktree(
        &state.db,
        &root.canonicalize().unwrap().display().to_string(),
    )
    .await
    .unwrap()
    .expect("registered worktree");
    let scan = ScanFileJob {
        worktree_id: worktree.id,
        identity: worktree.identity,
        path: "fresh.rs".to_string(),
        blob: fs3_git::blob_id(&file).unwrap().as_str().to_string(),
    };
    let scan_key = scan.dedupe_key();
    fs3_store::enqueue_job_with_priority(
        &state.db,
        roots::SCAN_FILE,
        &scan_key,
        &serde_json::to_value(&scan).expect("scan payload"),
        Duration::ZERO,
        fs3_store::JOB_PRIORITY_NEW_WORKTREE_SCAN,
    )
    .await
    .expect("queues promoted scan");

    let mut lock = state.db.begin().await.expect("starts blocker transaction");
    sqlx::query("LOCK TABLE worktrees IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *lock)
        .await
        .expect("locks scan lookup");

    let workers = state.config.indexing.worker_concurrency;
    let drain_state = state.clone();
    let draining = tokio::spawn(async move { runner::drain(&drain_state, workers).await });
    blocker.wait_for_calls(1).await;
    let scan_state: String = sqlx::query_scalar("SELECT state FROM jobs WHERE dedupe_key = $1")
        .bind(&scan_key)
        .fetch_one(&state.db)
        .await
        .expect("reads claimed scan");
    assert_eq!(scan_state, "running", "scan and summary both made progress");

    lock.commit().await.expect("releases scan lookup");
    blocker.release(1);
    tokio::time::timeout(Duration::from_secs(10), draining)
        .await
        .expect("mixed drain finishes")
        .expect("drain task does not panic");
    database.destroy(state.db.clone()).await;
}

/// A re-poll has its own claimant even while the general lane is blocked in a
/// provider call. The same proof also holds the ordering boundaries:
/// conversation turns keep source order under the canonical-GUID advisory lock,
/// while the serial enrichment lane resumes newest-first claim order after the
/// provider is released.
#[tokio::test]
async fn a_repoll_lands_before_an_enrichment_backlog_is_released() {
    const BACKLOG: usize = 32;
    const SESSION: &str = "d8d88bbc-3c0c-4c5d-ad6e-0040b8d3bcc0";

    let database = support::FreshDatabase::create("lane-ingest-starvation").await;
    let config = Config {
        database: DatabaseConfig {
            url: database.url(),
        },
        ..Config::default()
    };
    let mut state = AppState::from_config(config).expect("wires");
    fs3_store::migrate(&state.db).await.expect("migrates");

    // A real Claude session, first ingested to establish the cursor. The
    // second line is appended only after that cursor exists, making the queued
    // job below a re-poll rather than a first ingest wearing the same shape.
    let home = support::temp_dir("lane-ingest-home");
    let project = home.join(".claude/projects/-srv-work-repo");
    std::fs::create_dir_all(&project).expect("creates project tree");
    let session_file = project.join(format!("{SESSION}.jsonl"));
    std::fs::write(&session_file, claude_record(SESSION, 1, "first turn"))
        .expect("writes first turn");
    // SAFETY: this test binary has one HOME consumer; the other lane tests do
    // not read process configuration.
    unsafe { std::env::set_var("HOME", &home) };
    let request = IngestRequest {
        pij_id: None,
        session_id: Some(SESSION.to_string()),
        harness: Some("claude".to_string()),
        folder: Some("/srv/work/repo".to_string()),
    };
    let first = ingest(&state, &request).await.expect("first poll lands");
    assert_eq!(first.turns_new, 1, "the cursor starts after one turn");

    let blocker = Arc::new(BlockingSummarizer::default());
    state.summarizer = blocker.clone();
    let items = support::items(10_000..10_000 + BACKLOG as u32);
    support::hold(&state, "ingest-starvation", &items).await;
    let mut expected_lifo = Vec::with_capacity(BACKLOG);
    for (index, (raw_hash, text)) in items.iter().enumerate() {
        let address = format!("src/backlog.rs::f{index}");
        let element = Element::new(
            ElementKind::Function,
            "function_item",
            format!("f{index}"),
            address.clone(),
            Span::new(index as u32 + 1, index as u32 + 1),
            text,
        );
        let job = SummarizeJob {
            identity: "git:backlog".to_string(),
            raw_hash: raw_hash.clone(),
            element,
        };
        fs3_store::enqueue_job(
            &state.db,
            SUMMARIZE,
            &job.dedupe_key(),
            &serde_json::to_value(&job).expect("summarize payload"),
            Duration::ZERO,
        )
        .await
        .expect("queues backlog");
        expected_lifo.push(address);
    }
    expected_lifo.reverse();

    let runner = tokio::spawn(runner::run_forever(state.clone(), 1));
    blocker.wait_for_calls(1).await;
    assert_eq!(blocker.calls().len(), 1, "the provider lane is blocked");

    std::fs::OpenOptions::new()
        .append(true)
        .open(&session_file)
        .expect("opens session for append")
        .write_all(claude_record(SESSION, 2, "second turn").as_bytes())
        .expect("appends second turn");
    submit(&state, &request).await.expect("queues the re-poll");

    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let conversations = list(&state, &ListRequest::default())
                .await
                .expect("lists conversations");
            if conversations
                .conversations
                .iter()
                .any(|conversation| conversation.turns == 2)
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the re-poll lands while enrichment is blocked");
    assert_eq!(
        blocker.calls().len(),
        1,
        "no enrichment was released to make room for ingest"
    );

    blocker.release(BACKLOG);
    blocker.wait_for_calls(BACKLOG).await;
    tokio::time::timeout(Duration::from_secs(10), async {
        while fs3_store::jobs_remaining(&state.db)
            .await
            .expect("counts jobs")
            != 0
        {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("all enrichment settles after release");
    assert_eq!(
        blocker.calls(),
        expected_lifo,
        "the one-wide enrichment lane claims newest equal-priority work first"
    );

    runner.abort();
    let _ = runner.await;
    database.destroy(state.db.clone()).await;
}

async fn blocked_runner(
    label: &str,
    backlog: usize,
) -> (support::FreshDatabase, AppState, Arc<BlockingSummarizer>) {
    let database = support::FreshDatabase::create(label).await;
    let config = Config {
        database: DatabaseConfig {
            url: database.url(),
        },
        ..Config::default()
    };
    let mut state = AppState::from_config(config).expect("wires");
    fs3_store::migrate(&state.db).await.expect("migrates");
    let blocker = Arc::new(BlockingSummarizer::default());
    state.summarizer = blocker.clone();

    let items = support::items(20_000..20_000 + backlog as u32);
    support::hold(&state, label, &items).await;
    for (index, (raw_hash, text)) in items.iter().enumerate() {
        let element = Element::new(
            ElementKind::Function,
            "function_item",
            format!("f{index}"),
            format!("src/shutdown.rs::f{index}"),
            Span::new(index as u32 + 1, index as u32 + 1),
            text,
        );
        let job = SummarizeJob {
            identity: format!("git:{label}"),
            raw_hash: raw_hash.clone(),
            element,
        };
        fs3_store::enqueue_job(
            &state.db,
            SUMMARIZE,
            &job.dedupe_key(),
            &serde_json::to_value(&job).expect("summarize payload"),
            Duration::ZERO,
        )
        .await
        .expect("queues shutdown backlog");
    }

    (database, state, blocker)
}

#[tokio::test]
async fn first_shutdown_signal_finishes_in_flight_without_dequeueing_more() {
    const BACKLOG: usize = 4;
    let (database, state, blocker) = blocked_runner("shutdown-drain", BACKLOG).await;
    let (shutdown_tx, shutdown) = watch::channel(runner::Shutdown::Running);
    let handle = tokio::spawn(runner::run_until_shutdown(state.clone(), 1, shutdown));

    blocker.wait_for_calls(1).await;
    shutdown_tx
        .send(runner::Shutdown::Draining)
        .expect("runner observes shutdown");
    blocker.release(1);
    tokio::time::timeout(Duration::from_secs(10), handle)
        .await
        .expect("bounded drain finishes")
        .expect("runner task succeeds");

    assert_eq!(
        blocker.calls().len(),
        1,
        "queued work must remain untouched after the signal"
    );
    let rows = fs3_store::queue_depth(&state.db)
        .await
        .expect("queue depth");
    let pending: i64 = rows
        .iter()
        .filter(|row| row.kind == SUMMARIZE && row.state == "pending")
        .map(|row| row.depth)
        .sum();
    assert_eq!(pending, (BACKLOG - 1) as i64);
    database.destroy(state.db.clone()).await;
}

#[tokio::test]
async fn second_shutdown_signal_cancels_in_flight_without_claiming_more() {
    const BACKLOG: usize = 4;
    let (database, state, blocker) = blocked_runner("shutdown-force", BACKLOG).await;
    let (shutdown_tx, shutdown) = watch::channel(runner::Shutdown::Running);
    let handle = tokio::spawn(runner::run_until_shutdown(state.clone(), 1, shutdown));

    blocker.wait_for_calls(1).await;
    shutdown_tx
        .send(runner::Shutdown::Draining)
        .expect("runner observes first signal");
    shutdown_tx
        .send(runner::Shutdown::Forced)
        .expect("runner observes escalation");
    tokio::time::timeout(Duration::from_secs(10), handle)
        .await
        .expect("forced shutdown finishes promptly")
        .expect("runner task succeeds");

    assert_eq!(
        blocker.calls().len(),
        1,
        "no queued job may start after TERM"
    );
    let rows = fs3_store::queue_depth(&state.db)
        .await
        .expect("queue depth");
    let running: i64 = rows
        .iter()
        .filter(|row| row.kind == SUMMARIZE && row.state == "running")
        .map(|row| row.depth)
        .sum();
    assert_eq!(
        running, 1,
        "cancelled work remains recoverable at next boot"
    );
    database.destroy(state.db.clone()).await;
}

fn claude_record(session: &str, ordinal: u32, text: &str) -> String {
    format!(
        "{{\"type\":\"user\",\"uuid\":\"00000000-0000-4000-8000-{ordinal:012}\",\"parentUuid\":null,\"sessionId\":\"{session}\",\"cwd\":\"/srv/work/repo\",\"timestamp\":\"2026-08-28T06:00:{ordinal:02}Z\",\"message\":{{\"role\":\"user\",\"content\":[{{\"type\":\"text\",\"text\":\"{text}\"}}]}}}}\n"
    )
}
