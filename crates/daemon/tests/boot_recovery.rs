//! Boot's enrichment recovery must RUN, and must run in the right order.
//!
//! `retire_empty_embed_jobs` was built, unit-tested, and green in the store
//! crate while nothing in the daemon called it. That shape — a working
//! mechanism with no wired trigger — is the defect class this file exists to
//! close: tests that check a mechanism works do not check that anything
//! reaches it.
//!
//! Order is the other half. `requeue_failed` wakes every failed enrichment job
//! that is not `terminal`. Retiring the poison AFTER that sweep would hand the
//! empty-input jobs a fresh life on every boot and only close the door behind
//! them, and every assertion about the retirement itself would still pass.
//! So both tests here assert on what the OTHER half of the sequence did.

mod support;

use std::time::Duration;

use fs3_core::{Config, DatabaseConfig};
use fs3_daemon::boot;
use fs3_daemon::enrich;
use fs3_daemon::wiring::AppState;
use serde_json::json;

async fn stack(label: &str) -> (support::FreshDatabase, AppState) {
    let database = support::FreshDatabase::create(label).await;
    let config = Config {
        database: DatabaseConfig {
            url: database.url(),
        },
        ..Config::default()
    };
    let state = AppState::from_config(config).expect("the stack wires");
    fs3_store::migrate(&state.db).await.expect("migrates");
    (database, state)
}

/// Enqueue an embed job carrying `items`, claim it, and fail it — the exact
/// state the live poison rows were found in.
async fn failed_embed_job(state: &AppState, dedupe: &str, items: serde_json::Value) -> i64 {
    fs3_store::enqueue_job(
        &state.db,
        enrich::EMBED,
        dedupe,
        &json!({ "identity": "git:github.com/fs3/boot", "source": "raw", "items": items }),
        Duration::ZERO,
    )
    .await
    .expect("enqueues");

    let job = fs3_store::claim_job(&state.db, &[enrich::EMBED])
        .await
        .expect("claims")
        .expect("a job is ready");
    fs3_store::fail_job(
        &state.db,
        job.id,
        "provider_failed input cannot be an empty string",
        false,
    )
    .await
    .expect("fails");
    job.id
}

async fn state_of(state: &AppState, id: i64) -> (String, bool) {
    sqlx::query_as("SELECT state, terminal FROM jobs WHERE id = $1")
        .bind(id)
        .fetch_one(&state.db)
        .await
        .expect("the job is readable")
}

/// The whole point: boot's recovery sequence retires the poison and the sweep
/// that follows it in the same sequence does NOT revive it.
///
/// The control job is what makes this measurable. Without a job that SHOULD be
/// revived, a recovery that silently did nothing at all would pass every
/// assertion about the poison staying dead.
#[tokio::test]
async fn boot_recovery_retires_empty_embed_jobs_before_the_sweep_can_revive_them() {
    let (database, state) = stack("boot_recovery").await;

    let poison = failed_embed_job(&state, "embed:boot:poison", json!([["e3b0c442", ""]])).await;
    let whitespace = failed_embed_job(
        &state,
        "embed:boot:whitespace",
        json!([["ws0001", "   \n\t "]]),
    )
    .await;
    let control = failed_embed_job(
        &state,
        "embed:boot:control",
        json!([[
            "real0001",
            "fn handler(request: Request) -> Response { dispatch(request) }"
        ]]),
    )
    .await;

    for id in [poison, whitespace, control] {
        assert_eq!(
            state_of(&state, id).await,
            ("failed".to_string(), false),
            "every job starts failed and revivable, or this proves nothing"
        );
    }

    boot::recover_enrichment_jobs(&state.db).await;

    assert_eq!(
        state_of(&state, poison).await,
        ("failed".to_string(), true),
        "the empty-input job is retired and the sweep in the same sequence left it alone"
    );
    assert_eq!(
        state_of(&state, whitespace).await,
        ("failed".to_string(), true),
        "whitespace-only counts as empty, exactly as the mint filter counts it"
    );
    assert_eq!(
        state_of(&state, control).await,
        ("pending".to_string(), false),
        "a job with real text is still revived — the recovery ran, it did not merely do nothing"
    );

    database.destroy(state.db.clone()).await;
}

/// Recovery is idempotent across boots.
///
/// A daemon restarts. If the second pass could resurrect what the first
/// retired, the poison would return on a schedule and the receipt count would
/// read zero while the queue refilled.
#[tokio::test]
async fn a_second_boot_does_not_resurrect_what_the_first_retired() {
    let (database, state) = stack("boot_recovery_twice").await;

    let poison = failed_embed_job(&state, "embed:boot2:poison", json!([["e3b0c442", ""]])).await;

    boot::recover_enrichment_jobs(&state.db).await;
    boot::recover_enrichment_jobs(&state.db).await;

    assert_eq!(
        state_of(&state, poison).await,
        ("failed".to_string(), true),
        "still retired after a second boot"
    );

    database.destroy(state.db.clone()).await;
}
