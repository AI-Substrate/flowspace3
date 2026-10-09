//! `last_failure()` reports what is still broken, not what once was.
//!
//! Job 3237054 failed on 2026-09-25 and was still `status`'s `last_error` two
//! weeks later, after the same session had ingested cleanly every day since.
//! These pin the fix: a later success of the same kind and key retires a
//! failure, that survives the purge of the success, and a failure nothing has
//! fixed is still reported, with when it happened.

mod support;

use std::{num::NonZeroU32, time::Duration};

use fs3_store::PgPool;
use support::FreshDatabase;

const KIND: &str = "ingest_session";
const KEY: &str = "ingest:claude/a5a5588f@/srv/fs3-nul-turns";

async fn run(pool: &PgPool, kind: &str, key: &str, outcome: Result<(), &str>) -> i64 {
    let id = fs3_store::enqueue_job_id(pool, kind, key, &serde_json::json!({}), Duration::ZERO)
        .await
        .expect("enqueue");
    let claimed = fs3_store::claim_job(pool, &[kind])
        .await
        .expect("claim")
        .expect("the job just enqueued is ready");
    assert_eq!(claimed.id, id, "nothing else is queued in a fresh database");
    match outcome {
        Ok(()) => fs3_store::complete_job(pool, id).await.expect("complete"),
        Err(error) => fs3_store::fail_job(pool, id, error, true)
            .await
            .expect("fail"),
    }
    id
}

async fn backdate(pool: &PgPool, id: i64, days: i32) {
    sqlx::query("UPDATE jobs SET updated_at = now() - make_interval(days => $2) WHERE id = $1")
        .bind(id)
        .bind(days)
        .execute(pool)
        .await
        .expect("backdate");
}

#[tokio::test]
async fn a_failure_a_later_success_fixed_is_not_reported_even_after_the_success_is_purged() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;

    let failed = run(&pool, KIND, KEY, Err("FS3-E-STORE-QUERY-FAILED bad escape")).await;
    backdate(&pool, failed, 14).await;

    let before = fs3_store::last_failure(&pool)
        .await
        .expect("read")
        .expect("nothing has fixed it yet, so it is still the last error");
    assert_eq!(before.job_id, failed);
    assert_eq!(before.dedupe_key, KEY);
    assert!(before.at.ends_with('Z'), "UTC: {}", before.at);
    assert!(
        before.age_secs >= 14 * 86_400,
        "a two-week-old failure says so: {}",
        before.age_secs
    );

    let fixed = run(&pool, KIND, KEY, Ok(())).await;
    assert_ne!(
        fixed, failed,
        "a terminal failure keeps its row; the rerun is new"
    );
    assert_eq!(
        fs3_store::last_failure(&pool).await.expect("read"),
        None,
        "the same kind and key succeeded later, so the failure is history"
    );
    let superseded_by: Option<i64> =
        sqlx::query_scalar("SELECT superseded_by FROM jobs WHERE id = $1")
            .bind(failed)
            .fetch_one(&pool)
            .await
            .expect("read mark");
    assert_eq!(superseded_by, Some(fixed));

    let purged = fs3_store::purge_done_jobs(&pool, Duration::ZERO, NonZeroU32::MIN)
        .await
        .expect("purge");
    assert_eq!(purged, 1, "the success that proved the fix is gone");
    assert_eq!(
        fs3_store::last_failure(&pool).await.expect("read"),
        None,
        "retention must not bring a fixed failure back"
    );

    database.destroy(pool).await;
}

#[tokio::test]
async fn a_failure_nothing_has_fixed_is_reported_with_its_time_and_job() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;

    run(&pool, KIND, KEY, Ok(())).await;
    let failed = run(
        &pool,
        KIND,
        KEY,
        Err("FS3-E-STORE-QUERY-FAILED still broken"),
    )
    .await;
    // Same key, different kind: not the same work, so it proves nothing.
    run(&pool, "embed", KEY, Ok(())).await;

    let last = fs3_store::last_failure(&pool)
        .await
        .expect("read")
        .expect("a success BEFORE a failure does not fix it");
    assert_eq!(last.job_id, failed);
    assert_eq!(last.dedupe_key, KEY);
    assert!(last.error.contains("still broken"), "{}", last.error);
    assert!(last.at.ends_with('Z'), "UTC: {}", last.at);
    assert!(
        (0..60).contains(&last.age_secs),
        "it just failed: {}",
        last.age_secs
    );

    database.destroy(pool).await;
}
