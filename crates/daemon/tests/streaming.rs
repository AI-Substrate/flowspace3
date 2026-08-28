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

use fs3_core::{Config, DatabaseConfig};
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

/// A stack with a queue full of jobs that need no provider and no filesystem —
/// `embed` over inline texts, which the fake serves offline.
async fn stack_with_jobs(label: &str, jobs: usize) -> (support::FreshDatabase, AppState) {
    let database = support::FreshDatabase::create(label).await;
    let config = Config {
        database: DatabaseConfig {
            url: database.url(),
        },
        ..Config::default()
    };
    let state = AppState::from_config(config).expect("the fake stack wires");
    fs3_store::migrate(&state.db).await.expect("migrates");

    for n in 0..jobs {
        fs3_store::enqueue_job(
            &state.db,
            "embed",
            &format!("embed:test:{n}"),
            &json!({
                "identity": "git:test",
                "source": "raw",
                "items": [[format!("{n:040x}"), format!("body {n}")]],
            }),
            std::time::Duration::ZERO,
        )
        .await
        .expect("a job enqueues");
    }
    (database, state)
}

/// Every completion line must say how much is LEFT, and the numbers must fall
/// to zero.
///
/// Jordan, watching a live index: "I have no idea how far through I am." The
/// lines were there and each one was true, but a stream of facts with no
/// denominator is not a position. This is the denominator.
#[tokio::test]
async fn every_completion_line_says_how_much_is_left() {
    let (database, state) = stack_with_jobs("streaming_left", 6).await;
    let log = Captured::default();

    {
        let _guard = log.install();
        runner::drain(&state, 1).await;
    }

    let left: Vec<i64> = log
        .lines()
        .iter()
        .filter(|line| line.contains("runner: done"))
        .map(|line| {
            field(line, "left")
                .unwrap_or_else(|| panic!("a completion line with no position: {line}"))
                .parse()
                .expect("left is a number")
        })
        .collect();

    assert_eq!(left.len(), 6, "one line per job, each carrying a position");
    assert_eq!(
        left,
        vec![5, 4, 3, 2, 1, 0],
        "the count must fall to zero, and must EXCLUDE the job the line is \
         reporting — a line that says `1 left` when it is the last one is off \
         by one at the only moment anybody is still reading"
    );

    let pool = state.db.clone();
    database.destroy(pool).await;
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

    let done_lines = lines
        .iter()
        .filter(|line| line.contains("runner: done"))
        .count();
    assert_eq!(done_lines, 4, "and the per-job lines are still there");

    let pool = state.db.clone();
    database.destroy(pool).await;
}

/// `jobs_remaining` counts what is still to do, and nothing else.
///
/// Settled rows stay in the table forever — they are the run's history — so a
/// count that included them would climb while the backlog fell, which is the
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
/// [`stack_with_jobs`] enqueues exactly the shape the guard refuses: bare
/// hashes nothing maps. So a drain over it must narrate the refusal, and must
/// say HOW MUCH it refused rather than merely that it happened.
///
/// Summed across lines rather than read off one: the batch planner may merge
/// the three jobs into one call or not, and how many lines the guard prints is
/// its business. What it owes a reader is the total.
#[tokio::test]
async fn the_embed_spend_guard_says_what_it_refused_to_buy() {
    let (database, state) = stack_with_jobs("streaming_guard", 3).await;
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
