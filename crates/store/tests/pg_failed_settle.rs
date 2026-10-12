//! A failed job settles however long its provider's error is (DL-028).
//!
//! `last_error` is carried in `jobs_live_dedupe_idx`, and a btree entry must fit
//! in about 2.7 KB. On prod on 2026-10-11 a 3.7 KB provider error made Postgres
//! refuse the settle 1,125 times, and each refused job stayed `running`.

mod support;

use std::time::Duration;

use fs3_store::jobs::LAST_ERROR_MAX;
use serde_json::json;
use support::FreshDatabase;

/// A provider error body Postgres cannot compress into an index entry.
fn long_provider_error() -> String {
    let mut seed: u64 = 0x2545_f491_4f6c_dd1d;
    let noise: String = (0..4_000)
        .map(|_| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            char::from_digit((seed % 16) as u32, 16).expect("a hex digit")
        })
        .collect();
    format!("provider_error 400 maximum request size is 300000 tokens {noise}")
}

#[tokio::test]
async fn a_job_whose_error_is_longer_than_an_index_entry_still_settles() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;
    let error = long_provider_error();

    for key in ["retried", "failed"] {
        fs3_store::enqueue_job(&pool, "embed", key, &json!({}), Duration::ZERO)
            .await
            .expect("enqueues");
    }
    let claimed = fs3_store::claim_jobs(&pool, "embed", 2)
        .await
        .expect("claims both");
    let id_of = |key: &str| {
        claimed
            .iter()
            .find(|job| job.dedupe_key == key)
            .expect("claimed")
            .id
    };

    // Both stay in the live index: a retry is pending, a non-terminal failure
    // is revivable.
    fs3_store::retry_job(&pool, id_of("retried"), Duration::ZERO, &error)
        .await
        .expect("a retry with a long error is recorded");
    fs3_store::fail_job(&pool, id_of("failed"), &error, false)
        .await
        .expect("a failure with a long error is recorded");

    let rows: Vec<(String, String, String)> =
        sqlx::query_as("SELECT dedupe_key, state, last_error FROM jobs ORDER BY dedupe_key")
            .fetch_all(&pool)
            .await
            .expect("reads jobs");
    let settled: Vec<(&str, &str)> = rows
        .iter()
        .map(|(key, state, _)| (key.as_str(), state.as_str()))
        .collect();
    assert_eq!(settled, [("failed", "failed"), ("retried", "pending")]);
    for (key, _, last_error) in &rows {
        assert!(
            last_error.len() <= LAST_ERROR_MAX,
            "{key} keeps at most {LAST_ERROR_MAX} bytes, kept {}",
            last_error.len()
        );
        assert!(
            last_error.starts_with("provider_error 400 maximum request size"),
            "{key} keeps the start of the message: {last_error}"
        );
        assert!(
            last_error.ends_with(&format!("({} bytes in all)", error.len())),
            "{key} says it was cut: {last_error}"
        );
    }

    database.destroy(pool).await;
}
