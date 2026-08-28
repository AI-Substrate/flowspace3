//! Many embed jobs, one provider call, and every row settled on its own.
//!
//! The planner's rules are unit-tested in `runner::batch`. What this proves is
//! the part only a real queue can show: that k claimed jobs become ONE call,
//! that each job row is settled individually from it, and that a failed batch
//! puts every job it carried back rather than losing some and completing
//! others.

mod support;

use std::sync::Arc;

use fs3_core::{Config, DatabaseConfig};
use fs3_daemon::runner;
use fs3_daemon::wiring::AppState;
use fs3_testkit::fakes::FakeEmbedder;
use serde_json::json;
use sqlx::Row;

const IDENTITY: &str = "git:github.com/fs3/batch";

async fn stack(
    label: &str,
    embedder: Arc<FakeEmbedder>,
) -> (support::FreshDatabase, AppState, Arc<FakeEmbedder>) {
    let database = support::FreshDatabase::create(label).await;
    let config = Config {
        database: DatabaseConfig {
            url: database.url(),
        },
        ..Config::default()
    };
    let mut state = AppState::from_config(config).expect("wires");
    fs3_store::migrate(&state.db).await.expect("migrates");
    state.embedder = embedder.clone();
    (database, state, embedder)
}

fn working() -> Arc<FakeEmbedder> {
    Arc::new(FakeEmbedder {
        dimensions: fs3_store::EMBEDDING_DIMENSIONS,
        ..FakeEmbedder::default()
    })
}

/// Enqueue one embed job carrying a single item.
///
/// The item is the `n`th of [`support::items`], so [`support::hold`] over the
/// same range makes it referenced — without which the reference guard drops
/// the batch before the provider and every claim here settles as a free
/// success rather than as the merge it is testing.
async fn enqueue(state: &AppState, n: u32) {
    let (hash, text) = support::items(n..n + 1).remove(0);
    fs3_store::enqueue_job(
        &state.db,
        "embed",
        &format!("embed:batch:{n}"),
        &json!({
            "identity": IDENTITY,
            "source": "raw",
            "items": [[hash, text]],
        }),
        std::time::Duration::ZERO,
    )
    .await
    .expect("enqueues");
}

async fn states(state: &AppState) -> Vec<(String, i32)> {
    sqlx::query("SELECT state, attempts FROM jobs WHERE kind = 'embed' ORDER BY id")
        .fetch_all(&state.db)
        .await
        .expect("job rows")
        .iter()
        .map(|row| {
            (
                row.try_get::<String, _>("state").expect("state"),
                row.try_get::<i32, _>("attempts").expect("attempts"),
            )
        })
        .collect()
}

/// Eight jobs, one call, eight completed rows.
///
/// The whole point of multi-claim: the API takes many texts per request, and
/// the difference between one text per call and eight is most of the
/// throughput. Settling stays per-row, because the queue's unit of work is
/// still the job.
#[tokio::test]
async fn many_jobs_become_one_call_and_are_settled_individually() {
    let (database, state, embedder) = stack("batch_merge", working()).await;
    support::hold(&state, "batch-merge", &support::items(0..8)).await;
    for n in 0..8 {
        enqueue(&state, n).await;
    }

    let drained = runner::drain(&state, 2).await;

    assert_eq!(
        embedder.call_count(),
        1,
        "eight jobs, one provider call — merging is the point"
    );
    assert_eq!(
        embedder.calls.lock().expect("lock")[0].len(),
        8,
        "and all eight texts rode in it"
    );
    assert_eq!(drained.completed, 8, "settled as eight jobs, not one");
    assert!(
        states(&state)
            .await
            .iter()
            .all(|(state, _)| state == "done"),
        "every row done"
    );

    let pool = state.db.clone();
    database.destroy(pool).await;
}

/// When the merged call fails, EVERY job it carried goes back.
///
/// The failure mode this forbids is the ugly one: some rows completed and some
/// retried out of a single call that either happened or did not. A job marked
/// `done` for vectors that were never bought is a hole the reconciler cannot
/// see, because the queue's own memory says the work is finished.
#[tokio::test]
async fn a_failed_batch_puts_every_job_it_carried_back() {
    let embedder = Arc::new(FakeEmbedder {
        dimensions: fs3_store::EMBEDDING_DIMENSIONS,
        ..FakeEmbedder::failing_after(0)
    });
    let (database, state, _) = stack("batch_failure", embedder).await;
    support::hold(&state, "batch-failure", &support::items(0..4)).await;
    for n in 0..4 {
        enqueue(&state, n).await;
    }

    let drained = runner::drain(&state, 2).await;

    assert_eq!(drained.completed, 0, "nothing succeeded");
    assert_eq!(drained.retried, 4, "all four went back, not some of them");
    for (job_state, attempts) in states(&state).await {
        assert_eq!(job_state, "pending", "back on the queue");
        assert_eq!(attempts, 1, "one attempt spent, and only one");
    }

    let pool = state.db.clone();
    database.destroy(pool).await;
}

/// A job on its second attempt must not be able to take innocents with it.
///
/// One poisonous item fails the whole merged call. Without solo retry the jobs
/// beside it inherit that failure and burn their own attempts on somebody
/// else's bad data — so a single bad element could exhaust a whole batch.
#[tokio::test]
async fn a_suspect_job_is_embedded_alone() {
    let (database, state, embedder) = stack("batch_poison", working()).await;
    support::hold(&state, "batch-poison", &support::items(0..3)).await;
    for n in 0..3 {
        enqueue(&state, n).await;
    }
    // Job 2 has already failed once and gone back.
    sqlx::query("UPDATE jobs SET attempts = 1 WHERE dedupe_key = 'embed:batch:1'")
        .execute(&state.db)
        .await
        .expect("age one job");

    runner::drain(&state, 2).await;

    // Copied out and the guard dropped before the next await: a std Mutex held
    // across an await point is a deadlock waiting for a scheduler that moves
    // the task.
    let sizes: Vec<usize> = {
        let calls = embedder.calls.lock().expect("lock");
        assert_eq!(
            calls.len(),
            2,
            "the suspect gets its own call; the innocents share one"
        );
        let mut sizes: Vec<usize> = calls.iter().map(Vec::len).collect();
        sizes.sort_unstable();
        sizes
    };
    assert_eq!(sizes, vec![1, 2], "one alone, two together");

    let pool = state.db.clone();
    database.destroy(pool).await;
}
