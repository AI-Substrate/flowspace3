mod support;

use std::time::Duration;

use serde_json::json;
use support::FreshDatabase;

#[tokio::test]
async fn only_one_daemon_holds_a_database_and_the_hold_ends_with_it() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;

    let first = fs3_store::take_daemon_lock(&pool)
        .await
        .expect("takes the lock")
        .expect("nobody holds it yet");
    assert!(
        fs3_store::take_daemon_lock(&pool)
            .await
            .expect("asks for the lock")
            .is_none(),
        "a second daemon on the same database is refused"
    );

    drop(first);
    // The session closes asynchronously once the connection is dropped.
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let again = loop {
        if let Some(lock) = fs3_store::take_daemon_lock(&pool)
            .await
            .expect("asks for the lock")
        {
            break lock;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the lock outlived the daemon that held it"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    drop(again);
    database.destroy(pool).await;
}

#[tokio::test]
async fn releasing_at_shutdown_makes_claims_claimable_and_refunds_the_attempt() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;

    for (kind, key) in [("embed", "e1"), ("embed", "e2"), ("summarize", "s1")] {
        fs3_store::enqueue_job(&pool, kind, key, &json!({}), Duration::ZERO)
            .await
            .expect("enqueues");
    }
    fs3_store::enqueue_job(&pool, "scan_file", "untouched", &json!({}), Duration::ZERO)
        .await
        .expect("enqueues");
    let claimed = fs3_store::claim_jobs(&pool, "embed", 2)
        .await
        .expect("claims embeds");
    assert_eq!(claimed.len(), 2);
    let summarize = fs3_store::claim_job(&pool, &["summarize"])
        .await
        .expect("claims a summary")
        .expect("one is ready");
    assert_eq!(summarize.attempts, 1, "a claim spends an attempt");

    let released = fs3_store::release_running(&pool)
        .await
        .expect("releases claims");
    assert_eq!(
        released,
        vec![("embed".to_string(), 2), ("summarize".to_string(), 1)]
    );

    let rows: Vec<(String, String, i32)> =
        sqlx::query_as("SELECT dedupe_key, state, attempts FROM jobs ORDER BY dedupe_key")
            .fetch_all(&pool)
            .await
            .expect("reads jobs");
    for (key, state, attempts) in &rows {
        assert_eq!(state, "pending", "{key} is claimable again");
        assert_eq!(*attempts, 0, "{key} has its attempt back");
    }

    let reclaimed = fs3_store::claim_job(&pool, &["summarize"])
        .await
        .expect("claims again")
        .expect("the released summary is ready at once");
    assert_eq!(reclaimed.id, summarize.id);
    assert_eq!(reclaimed.attempts, 1, "the re-claim is its first attempt");

    database.destroy(pool).await;
}
