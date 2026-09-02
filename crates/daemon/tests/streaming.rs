//! What a watcher SEES while an index runs.
//!
//! The pipeline being correct is proven elsewhere (`first_light.rs`). What this
//! binary defends is the other half of the claim: that someone watching the
//! daemon can tell what it is doing and how far through it is. That is a
//! behaviour with a consumer — a human reading a terminal — and it has now
//! twice been wrong in ways the pipeline tests could not see, because a silent
//! daemon and a working one produce identical rows.
//!
//! So these tests read the LOG LINES, captured off the real runner running real
//! jobs, rather than asserting on the counters behind them.

mod support;

use std::sync::{Arc, Mutex};

use fs3_core::{Config, DatabaseConfig, EventKind};
use fs3_daemon::Reconcile;
use fs3_daemon::retention::RetentionSupervisor;
use fs3_daemon::runner;
use fs3_daemon::wiring::AppState;
use serde_json::json;
use tracing::subscriber::DefaultGuard;
use tracing_subscriber::fmt::MakeWriter;

/// A writer that keeps everything written to it, so a test can read the log a
/// human would have read.
#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Captured {
    /// Install as the subscriber for THIS thread only, so tests running in
    /// parallel cannot read each other's lines.
    fn install(&self) -> DefaultGuard {
        let subscriber = tracing_subscriber::fmt()
            .with_writer(self.clone())
            .with_max_level(tracing::Level::INFO)
            .with_ansi(false)
            .finish();
        tracing::subscriber::set_default(subscriber)
    }

    fn lines(&self) -> Vec<String> {
        String::from_utf8(self.0.lock().expect("the log is not poisoned").clone())
            .expect("log output is utf-8")
            .lines()
            .map(str::to_string)
            .collect()
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

/// The value of a `key=value` field on a log line, if it has one.
fn field(line: &str, key: &str) -> Option<String> {
    let raw = line
        .split_whitespace()
        .find_map(|token| token.strip_prefix(&format!("{key}=")))?;
    // tracing quotes string fields and leaves numbers bare.
    Ok::<_, ()>(raw.trim_matches('"').to_string()).ok()
}

/// A stack with a queue full of held embed jobs served by the offline fake.
async fn stack_with_jobs(label: &str, jobs: usize) -> (support::FreshDatabase, AppState) {
    stack_with_jobs_inner(label, jobs, true).await
}

/// The spend-guard twin: identical jobs whose texts no root holds.
async fn stack_with_unheld_jobs(label: &str, jobs: usize) -> (support::FreshDatabase, AppState) {
    stack_with_jobs_inner(label, jobs, false).await
}

async fn stack_with_jobs_inner(
    label: &str,
    jobs: usize,
    held: bool,
) -> (support::FreshDatabase, AppState) {
    let database = support::FreshDatabase::create(label).await;
    let config = Config {
        database: DatabaseConfig {
            url: database.url(),
        },
        ..Config::default()
    };
    let state = AppState::from_config(config).expect("the fake stack wires");
    fs3_store::migrate(&state.db).await.expect("migrates");

    let items = support::items(0..u32::try_from(jobs).expect("small test batch"));
    if held {
        support::hold(&state, label, &items).await;
    }
    for (n, (hash, text)) in items.into_iter().enumerate() {
        fs3_store::enqueue_job(
            &state.db,
            "embed",
            &format!("embed:test:{n}"),
            &json!({
                "identity": "git:test",
                "source": "raw",
                "items": [[hash, text]],
            }),
            std::time::Duration::ZERO,
        )
        .await
        .expect("a job enqueues");
    }
    (database, state)
}

/// Provider work is reported once for the merged batch, not once per queue row.
/// The five-second progress rollup owns position; provider lines own request
/// shape, outcome, and duration.
#[tokio::test]
async fn embed_provider_calls_are_reported_as_groups() {
    let (database, state) = stack_with_jobs("streaming_grouped", 6).await;
    let log = Captured::default();

    {
        let _guard = log.install();
        runner::drain(&state, 1).await;
    }

    let lines = log.lines();
    let calls: Vec<&String> = lines
        .iter()
        .filter(|line| line.contains("embed: sent batch"))
        .collect();
    assert_eq!(
        calls.len(),
        1,
        "six jobs merge into one provider line: {lines:#?}"
    );
    let call = calls[0];
    assert_eq!(field(call, "items").as_deref(), Some("6"));
    assert_eq!(field(call, "source").as_deref(), Some("raw"));
    assert_eq!(field(call, "outcome").as_deref(), Some("ok"));
    assert!(field(call, "ms").is_some(), "duration is present: {call}");
    assert!(
        lines.iter().all(|line| !line.contains("runner: done")),
        "per-item completion lines stay below INFO: {lines:#?}"
    );

    database.destroy(state.db.clone()).await;
}

/// The periodic summary must appear WHILE work is happening.
///
/// It used to be printed by the caller's loop, between drains — but `drain`
/// returns only when nothing is ready, so a busy queue never left it, and the
/// summary that exists to narrate a long run was the one thing a long run
/// never printed. It only ever showed up on short queues, which is why the
/// tests and the demo both looked fine.
#[tokio::test]
async fn progress_is_reported_while_the_queue_is_still_draining() {
    let (database, state) = stack_with_jobs("streaming_progress", 4).await;
    let log = Captured::default();

    {
        let _guard = log.install();
        runner::drain(&state, 1).await;
    }

    let lines = log.lines();
    let progress: Vec<&String> = lines
        .iter()
        .filter(|line| line.contains("runner: progress"))
        .collect();

    assert!(
        !progress.is_empty(),
        "a drain must narrate itself, not only its ending: {lines:#?}"
    );
    let first = progress.first().expect("a progress line");
    assert_eq!(
        field(first, "phase").as_deref(),
        Some("working"),
        "reported from inside the drain, so the phase is the truth: {first}"
    );
    for historical in ["scanned", "summarized", "embedded"] {
        assert!(
            field(first, historical).is_none(),
            "the hot-path census must not derive {historical} from done history: {first}"
        );
    }

    let provider_lines = lines
        .iter()
        .filter(|line| line.contains("embed: sent batch"))
        .count();
    assert_eq!(provider_lines, 1, "the four jobs share one provider line");
    assert!(
        lines.iter().all(|line| !line.contains("runner: done")),
        "per-item completion lines stay below INFO"
    );

    let pool = state.db.clone();
    database.destroy(pool).await;
}

#[tokio::test]
async fn status_retention_log_names_completed_purge() {
    let (database, state) = stack_with_jobs("status_retention_log", 0).await;
    sqlx::query(
        "INSERT INTO jobs (kind, dedupe_key, payload, state, updated_at)
         VALUES ('scan_file', 'expired-log-row', '{}'::jsonb, 'done',
                 now() - interval '2 days')",
    )
    .execute(&state.db)
    .await
    .expect("seed expired job");
    let mut supervisor = RetentionSupervisor::new(state.db.clone(), 1);
    let log = Captured::default();

    let pass = {
        let _guard = log.install();
        supervisor.reconcile().await.expect("retention pass")
    };

    assert_eq!(pass.changed, 1);
    let lines = log.lines();
    let receipt = lines
        .iter()
        .find(|line| line.contains("purged expired done jobs"))
        .unwrap_or_else(|| panic!("retention receipt missing: {lines:#?}"));
    assert!(receipt.contains("window_days=1"), "{receipt}");
    assert!(receipt.contains("purged=1"), "{receipt}");

    database.destroy(state.db.clone()).await;
}

/// Queue censuses are reporting work, not settlement work.
///
/// A fast drain finishes inside one five-second window. Its number of grouped
/// snapshots therefore stays fixed while its per-job completion events scale
/// with the number of settled rows. The final snapshot must also observe the
/// terminal zero so a watcher never stops on stale in-flight counts.
#[tokio::test]
async fn queue_snapshots_follow_reporting_cadence_not_settlements() {
    const JOBS: usize = 24;
    const HISTORY: i64 = 50_000;
    let (database, state) = stack_with_jobs("streaming_queue_cadence", JOBS).await;
    sqlx::query(
        "INSERT INTO jobs (kind, dedupe_key, payload, state) \
         SELECT 'embed', 'history:' || n, '{}'::jsonb, 'done' \
         FROM generate_series(1, $1) AS n",
    )
    .bind(HISTORY)
    .execute(&state.db)
    .await
    .expect("seeds a large settled history");
    let mut events = state.subscribe();

    runner::drain(&state, 1).await;

    let mut completed = 0;
    let mut snapshots = Vec::new();
    while let Ok(event) = events.try_recv() {
        match event.kind {
            EventKind::JobDone { job, .. } if job == "embed" => completed += 1,
            EventKind::Queue { rows } => snapshots.push(rows),
            _ => {}
        }
    }

    assert_eq!(completed, JOBS, "one observable completion per settled row");
    assert_eq!(
        snapshots.len(),
        2,
        "one working snapshot plus one final snapshot, not one census per job"
    );
    let final_rows = snapshots
        .last()
        .expect("the drain publishes terminal state");
    assert_eq!(
        final_rows
            .iter()
            .filter(|row| row.state == "pending" || row.state == "running")
            .map(|row| row.count)
            .sum::<i64>(),
        0,
        "the final snapshot observes an empty live queue"
    );
    assert!(
        final_rows.iter().all(|row| row.state != "done"),
        "queue events are live snapshots, not a scan of {HISTORY} settled rows: {final_rows:#?}"
    );

    let pool = state.db.clone();
    database.destroy(pool).await;
}

/// `jobs_remaining` counts what is still to do, and nothing else.
///
/// Settled rows live longer than active work but are not part of the backlog.
/// A count that included them would climb while the queue fell, which is the
/// exact inversion of what the number is for.
#[tokio::test]
async fn remaining_counts_live_work_and_ignores_settled_history() {
    let (database, state) = stack_with_jobs("streaming_remaining", 3).await;

    assert_eq!(
        fs3_store::jobs_remaining(&state.db).await.expect("counts"),
        3
    );

    let job = fs3_store::claim_job(&state.db, &["embed"])
        .await
        .expect("claims")
        .expect("a job is ready");
    assert_eq!(
        fs3_store::jobs_remaining(&state.db).await.expect("counts"),
        3,
        "a RUNNING job is still work to do"
    );

    fs3_store::complete_job(&state.db, job.id)
        .await
        .expect("completes");
    assert_eq!(
        fs3_store::jobs_remaining(&state.db).await.expect("counts"),
        2,
        "a done job is history"
    );

    let job = fs3_store::claim_job(&state.db, &["embed"])
        .await
        .expect("claims")
        .expect("a job is ready");
    fs3_store::fail_job(&state.db, job.id, "no", true)
        .await
        .expect("fails");
    assert_eq!(
        fs3_store::jobs_remaining(&state.db).await.expect("counts"),
        1,
        "a terminally failed job is not work anybody is waiting for"
    );

    let pool = state.db.clone();
    database.destroy(pool).await;
}

/// Money NOT spent has to say so.
///
/// The reference guard drops embed items for content no registered root holds.
/// A guard that works silently is indistinguishable from a provider that was
/// never going to be called: the rows are identical either way — no vectors,
/// no failures, jobs completed — and the only place the saving exists at all
/// is the log. That is precisely this binary's subject.
///
/// [`stack_with_unheld_jobs`] enqueues exactly the shape the guard refuses:
/// bare hashes nothing maps. So a drain over it must narrate the refusal and
/// say HOW MUCH it refused rather than merely that it happened.
///
/// Summed across lines rather than read off one: the batch planner may merge
/// the three jobs into one call or not, and how many lines the guard prints is
/// its business. What it owes a reader is the total.
#[tokio::test]
async fn the_embed_spend_guard_says_what_it_refused_to_buy() {
    let (database, state) = stack_with_unheld_jobs("streaming_guard", 3).await;
    let log = Captured::default();

    {
        let _guard = log.install();
        runner::drain(&state, 1).await;
    }

    let lines = log.lines();
    let guard: Vec<&String> = lines
        .iter()
        .filter(|line| line.contains("skipping embeds for content no registered root holds"))
        .collect();
    assert!(
        !guard.is_empty(),
        "the guard saved three provider inputs and said nothing about it: {lines:#?}"
    );

    let total = |key: &str| -> i64 {
        guard
            .iter()
            .map(|line| {
                field(line, key)
                    .unwrap_or_else(|| panic!("a guard line with no {key}: {line}"))
                    .parse::<i64>()
                    .expect("a count is a number")
            })
            .sum()
    };

    assert_eq!(
        total("dropped"),
        3,
        "every unheld item is counted: {guard:#?}"
    );
    assert_eq!(
        total("kept"),
        0,
        "and nothing survived to be bought: {guard:#?}"
    );
    assert_eq!(
        field(guard[0], "kind").as_deref(),
        Some("raw"),
        "the kind is named, because raw and smart hashes live in different \
         spaces and a reader chasing a bill needs to know which one: {}",
        guard[0]
    );

    let pool = state.db.clone();
    database.destroy(pool).await;
}
