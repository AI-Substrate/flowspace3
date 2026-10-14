//! Table maintenance: autovacuum thresholds that fire on big tables, and the
//! health read doctor and the daemon's ANALYZE pass rely on (2026-10-14).

mod support;

use support::FreshDatabase;

#[tokio::test]
async fn migration_0031_sets_autovacuum_thresholds_on_every_watched_table() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;
    for table in fs3_store::WATCHED_TABLES {
        let options: Vec<String> = sqlx::query_scalar(
            "SELECT unnest(reloptions) FROM pg_class
              WHERE relname = $1 AND relnamespace = current_schema()::regnamespace",
        )
        .bind(table)
        .fetch_all(&pool)
        .await
        .expect("reads reloptions");
        // The vector table vacuums its HNSW index too, so it vacuums less
        // often; every table analyzes at 1%.
        let vacuum = if table == "embeddings_1024" {
            "0.10"
        } else {
            "0.02"
        };
        assert!(
            options.contains(&format!("autovacuum_vacuum_scale_factor={vacuum}"))
                && options.contains(&"autovacuum_analyze_scale_factor=0.01".to_string()),
            "{table}: {options:?}"
        );
        if table == "embeddings_1024" || table == "elements" {
            assert!(
                options.contains(&"autovacuum_vacuum_cost_delay=10".to_string()),
                "{table} vacuums slowly: {options:?}"
            );
        }
    }
    database.destroy(pool).await;
}

#[tokio::test]
async fn table_health_reads_every_watched_table_and_analyze_refreshes_one() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;

    let health = fs3_store::table_health(&pool).await.expect("reads health");
    let mut tables: Vec<&str> = health.iter().map(|table| table.table.as_str()).collect();
    tables.sort_unstable();
    let mut watched = fs3_store::WATCHED_TABLES.to_vec();
    watched.sort_unstable();
    assert_eq!(tables, watched);

    fs3_store::analyze_table(&pool, "elements")
        .await
        .expect("a watched table can be analyzed");
    assert!(
        fs3_store::analyze_table(&pool, "elements; DROP TABLE jobs")
            .await
            .is_err(),
        "only watched table names reach the statement"
    );
    database.destroy(pool).await;
}
