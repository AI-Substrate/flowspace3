mod support;

use std::{num::NonZeroU32, time::Duration};

use fs3_store::jobs::LIVE_QUEUE_DEPTH_SQL;
use serde_json::Value;
use support::FreshDatabase;

const OLD_QUEUE_DEPTH_SQL: &str = "SELECT kind, state, count(*) AS depth,
            count(*) FILTER (WHERE last_error IS NOT NULL) AS with_error
       FROM jobs
      GROUP BY kind, state
      ORDER BY kind, state";

fn has_job_node(plan: &Value, node_type: &str, index: Option<&str>) -> bool {
    match plan {
        Value::Array(values) => values
            .iter()
            .any(|value| has_job_node(value, node_type, index)),
        Value::Object(fields) => {
            let matches = fields.get("Node Type").and_then(Value::as_str) == Some(node_type)
                && fields.get("Relation Name").and_then(Value::as_str) == Some("jobs")
                && index.is_none_or(|name| {
                    fields.get("Index Name").and_then(Value::as_str) == Some(name)
                });
            matches
                || fields
                    .values()
                    .any(|value| has_job_node(value, node_type, index))
        }
        _ => false,
    }
}

async fn explain(pool: &fs3_store::PgPool, statement: &str) -> Value {
    let sql = format!("EXPLAIN (FORMAT JSON) {statement}");
    sqlx::query_scalar(&sql)
        .fetch_one(pool)
        .await
        .expect("query plan should be available")
}

#[tokio::test]
async fn queue_depth_plan_is_live_only_and_never_scans_done_history() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;

    sqlx::query(
        "INSERT INTO jobs (kind, dedupe_key, payload, state, terminal, last_error)
         SELECT 'scan_file', 'done:' || n, '{}'::jsonb, 'done', false, NULL
           FROM generate_series(1, 200000) AS n",
    )
    .execute(&pool)
    .await
    .expect("seed prod-shaped done history");
    sqlx::query(
        "INSERT INTO jobs (kind, dedupe_key, payload, state, terminal, last_error)
         VALUES ('scan_file', 'pending:1', '{}'::jsonb, 'pending', false, NULL),
                ('summarize', 'running:1', '{}'::jsonb, 'running', false, NULL),
                ('embed', 'failed:1', '{}'::jsonb, 'failed', false, 'retryable'),
                ('embed', 'terminal:1', '{}'::jsonb, 'failed', true, 'hopeless')",
    )
    .execute(&pool)
    .await
    .expect("seed every live-state boundary");
    sqlx::query("ANALYZE jobs")
        .execute(&pool)
        .await
        .expect("planner sees the prod-shaped population");

    let rows = fs3_store::queue_depth(&pool)
        .await
        .expect("live queue depth");
    assert_eq!(rows.iter().map(|row| row.depth).sum::<i64>(), 3);
    assert!(rows.iter().all(|row| row.state != "done"));
    assert!(
        rows.iter()
            .all(|row| row.state != "failed" || row.kind == "embed")
    );

    let live_plan = explain(&pool, LIVE_QUEUE_DEPTH_SQL).await;
    assert!(
        !has_job_node(&live_plan, "Seq Scan", None),
        "live query regressed to a jobs Seq Scan: {live_plan:#}"
    );
    assert!(
        has_job_node(&live_plan, "Index Only Scan", Some("jobs_live_dedupe_idx")),
        "live query must stay index-only: {live_plan:#}"
    );

    let purge_plan = explain(
        &pool,
        "SELECT id FROM jobs
          WHERE state = 'done'
            AND updated_at < now() - interval '1 day'
          ORDER BY updated_at, id
          LIMIT 10000
          FOR UPDATE SKIP LOCKED",
    )
    .await;
    assert!(
        has_job_node(&purge_plan, "Index Scan", Some("jobs_done_retention_idx")),
        "bounded purge must start from the retention index: {purge_plan:#}"
    );

    let failure_plan = explain(
        &pool,
        "SELECT dedupe_key, last_error FROM jobs
          WHERE state = 'failed' AND last_error IS NOT NULL
          ORDER BY updated_at DESC LIMIT 1",
    )
    .await;
    assert!(
        has_job_node(&failure_plan, "Index Scan", Some("jobs_failed_recent_idx")),
        "latest failure must use its ordered partial index: {failure_plan:#}"
    );

    let old_plan = explain(&pool, OLD_QUEUE_DEPTH_SQL).await;
    assert!(
        has_job_node(&old_plan, "Seq Scan", None),
        "mutation check: the old unfiltered GROUP BY must be red: {old_plan:#}"
    );

    database.destroy(pool).await;
}

async fn sweep(pool: &fs3_store::PgPool, older_than: Duration, batch: NonZeroU32) -> u64 {
    let mut total = 0;
    loop {
        let purged = fs3_store::purge_done_jobs(pool, older_than, batch)
            .await
            .expect("bounded purge");
        assert!(
            purged <= u64::from(batch.get()),
            "one statement exceeded its batch"
        );
        total += purged;
        if purged < u64::from(batch.get()) {
            return total;
        }
    }
}

#[tokio::test]
async fn retention_purges_only_aged_done_rows_in_bounded_idempotent_batches() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;

    sqlx::query(
        "INSERT INTO jobs (kind, dedupe_key, payload, state, terminal, updated_at)
         SELECT 'scan_file', 'old-done:' || n, '{}'::jsonb, 'done', false,
                now() - interval '2 days'
           FROM generate_series(1, 3) AS n",
    )
    .execute(&pool)
    .await
    .expect("seed expired done jobs");
    sqlx::query(
        "INSERT INTO jobs (kind, dedupe_key, payload, state, terminal, updated_at)
         VALUES ('scan_file', 'young-done', '{}'::jsonb, 'done', false, now()),
                ('scan_file', 'pending-live', '{}'::jsonb, 'pending', false, now() - interval '2 days'),
                ('scan_file', 'running-live', '{}'::jsonb, 'running', false, now() - interval '2 days'),
                ('scan_file', 'failed-live', '{}'::jsonb, 'failed', false, now() - interval '2 days'),
                ('scan_file', 'failed-terminal', '{}'::jsonb, 'failed', true, now() - interval '2 days')",
    )
    .execute(&pool)
    .await
    .expect("seed every protected boundary");

    let batch = NonZeroU32::new(2).unwrap();
    assert_eq!(
        sweep(&pool, Duration::from_secs(86_400), batch).await,
        3,
        "three expired rows require more than one two-row statement"
    );
    assert_eq!(
        sweep(&pool, Duration::from_secs(86_400), batch).await,
        0,
        "a complete second sweep is idempotent"
    );

    let survivors: Vec<String> =
        sqlx::query_scalar("SELECT dedupe_key FROM jobs ORDER BY dedupe_key")
            .fetch_all(&pool)
            .await
            .expect("read survivors");
    assert_eq!(
        survivors,
        [
            "failed-live",
            "failed-terminal",
            "pending-live",
            "running-live",
            "young-done",
        ]
    );

    database.destroy(pool).await;
}

#[tokio::test]
async fn dedupe_failed_non_terminal_job_absorbs_a_claimable_refire() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;
    let key = "scan:failed-owner";

    fs3_store::enqueue_job(
        &pool,
        "scan_file",
        key,
        &serde_json::json!({"attempt": 1}),
        Duration::ZERO,
    )
    .await
    .expect("initial mint");
    let claimed = fs3_store::claim_job(&pool, &["scan_file"])
        .await
        .expect("claim query")
        .expect("the job is ready");
    fs3_store::fail_job(&pool, claimed.id, "retry later", false)
        .await
        .expect("non-terminal failure");
    sqlx::query("UPDATE jobs SET attempts = 3, parks = 20 WHERE id = $1")
        .bind(claimed.id)
        .execute(&pool)
        .await
        .expect("seed exhausted retry and park budgets");

    fs3_store::enqueue_job(
        &pool,
        "scan_file",
        key,
        &serde_json::json!({"attempt": 2}),
        Duration::ZERO,
    )
    .await
    .expect("the re-fire revives its existing owner");

    let rows: Vec<(i64, String, i32, i32, serde_json::Value)> = sqlx::query_as(
        "SELECT id, state, attempts, parks, payload FROM jobs WHERE dedupe_key = $1",
    )
    .bind(key)
    .fetch_all(&pool)
    .await
    .expect("read the dedupe owner");
    assert_eq!(rows.len(), 1, "one key has one active owner: {rows:#?}");
    assert_eq!(rows[0].0, claimed.id, "the failed row remains the owner");
    assert_eq!(rows[0].1, "pending");
    assert_eq!(rows[0].2, 0, "the retry budget is fresh");
    assert_eq!(rows[0].3, 0, "the park budget is fresh");
    assert_eq!(rows[0].4, serde_json::json!({"attempt": 2}));

    let revived = fs3_store::claim_job(&pool, &["scan_file"])
        .await
        .expect("claim revived row")
        .expect("an absorbed re-fire must be claimable");
    assert_eq!(revived.id, claimed.id);
    assert_eq!(revived.attempts, 1);
    assert_eq!(revived.parks, 0);

    database.destroy(pool).await;
}

#[tokio::test]
async fn dedupe_running_and_terminal_failed_rows_keep_their_distinct_semantics() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;

    let running_key = "scan:running-owner";
    fs3_store::enqueue_job(
        &pool,
        "scan_file",
        running_key,
        &serde_json::json!({"version": 1}),
        Duration::ZERO,
    )
    .await
    .expect("mint running control");
    let running = fs3_store::claim_job(&pool, &["scan_file"])
        .await
        .expect("claim running control")
        .expect("running control is ready");
    sqlx::query("UPDATE jobs SET attempts = 7, parks = 9 WHERE id = $1")
        .bind(running.id)
        .execute(&pool)
        .await
        .expect("seed running budgets");
    fs3_store::enqueue_job(
        &pool,
        "scan_file",
        running_key,
        &serde_json::json!({"version": 2}),
        Duration::ZERO,
    )
    .await
    .expect("a running re-fire is absorbed without demotion");
    let running_after: (i64, String, i32, i32) =
        sqlx::query_as("SELECT id, state, attempts, parks FROM jobs WHERE dedupe_key = $1")
            .bind(running_key)
            .fetch_one(&pool)
            .await
            .expect("read running owner");
    assert_eq!(running_after, (running.id, "running".to_string(), 7, 9));
    fs3_store::complete_job(&pool, running.id)
        .await
        .expect("settle running control");

    let terminal_key = "scan:terminal-history";
    fs3_store::enqueue_job(
        &pool,
        "scan_file",
        terminal_key,
        &serde_json::json!({"version": 1}),
        Duration::ZERO,
    )
    .await
    .expect("mint terminal control");
    let terminal = fs3_store::claim_job(&pool, &["scan_file"])
        .await
        .expect("claim terminal control")
        .expect("terminal control is ready");
    fs3_store::fail_job(&pool, terminal.id, "hopeless", true)
        .await
        .expect("terminal failure");
    fs3_store::enqueue_job(
        &pool,
        "scan_file",
        terminal_key,
        &serde_json::json!({"version": 2}),
        Duration::ZERO,
    )
    .await
    .expect("terminal history does not absorb fresh work");

    let terminal_rows: Vec<(i64, String, bool)> =
        sqlx::query_as("SELECT id, state, terminal FROM jobs WHERE dedupe_key = $1 ORDER BY id")
            .bind(terminal_key)
            .fetch_all(&pool)
            .await
            .expect("read terminal control rows");
    assert_eq!(terminal_rows.len(), 2);
    assert_eq!(terminal_rows[0], (terminal.id, "failed".to_string(), true));
    assert_eq!(terminal_rows[1].1, "pending");
    assert!(!terminal_rows[1].2);
    assert_ne!(terminal_rows[1].0, terminal.id);
    let fresh = fs3_store::claim_job(&pool, &["scan_file"])
        .await
        .expect("claim fresh row")
        .expect("fresh row beside terminal history is claimable");
    assert_ne!(fresh.id, terminal.id);
    assert_eq!(fresh.dedupe_key, terminal_key);

    database.destroy(pool).await;
}
