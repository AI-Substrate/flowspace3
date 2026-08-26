//! What the migration story has to be true for: a fresh database becomes a
//! working store, and running it again changes nothing.
//!
//! `pg_round_trip.rs` migrates the *shared* 5433 database, which by the second
//! run is never fresh — so it can only ever prove the no-op half by accident.
//! These tests create a throwaway database per test and prove both halves on
//! purpose. Like every store test, they need the dockerized Postgres and fail
//! naming the command rather than skipping.

mod support;

use fs3_store::{MIGRATOR, PgPool, migrate};
use support::FreshDatabase;

/// Every version the binary carries, in order. Asserting against the embedded
/// set rather than a hardcoded `[1]` keeps this test honest when `0006` lands.
fn embedded_versions() -> Vec<i64> {
    MIGRATOR.iter().map(|migration| migration.version).collect()
}

async fn applied_rows(pool: &PgPool) -> Vec<(i64, String, Vec<u8>, bool)> {
    // `installed_on` is a TIMESTAMPTZ and sqlx has no date/time feature enabled
    // here, so it comes back as text — which is all this needs it for.
    sqlx::query_as("SELECT version, installed_on::text, checksum, success FROM _sqlx_migrations ORDER BY version")
        .fetch_all(pool)
        .await
        .expect("the migration tracking table should be readable")
}

/// Does the named relation exist?
async fn relation(pool: &PgPool, name: &str) -> Option<String> {
    sqlx::query_scalar("SELECT to_regclass($1)::text")
        .bind(format!("public.{name}"))
        .fetch_one(pool)
        .await
        .expect("asking for the relation should succeed")
}

/// A database with nothing in it becomes a usable store.
#[tokio::test]
async fn migrating_an_empty_database_bootstraps_the_whole_schema() {
    let database = FreshDatabase::create().await;
    let pool = database.pool().await;

    migrate(&pool)
        .await
        .expect("a fresh database should migrate");

    let versions: Vec<i64> = applied_rows(&pool)
        .await
        .into_iter()
        .map(|(version, _, _, _)| version)
        .collect();
    assert_eq!(
        versions,
        embedded_versions(),
        "every embedded migration should be recorded as applied"
    );

    // Workshop 002's three layers, named one by one. A migration that silently
    // half-applied would still record its version, so the version set alone is
    // not evidence that the schema is there.
    for table in [
        "repos",
        "worktrees",
        "worktree_files",
        "elements",
        "smart_content",
        "embeddings_1024",
        "jobs",
    ] {
        assert_eq!(
            relation(&pool, table).await.as_deref(),
            Some(table),
            "the workshop 002 schema is incomplete without {table}"
        );
    }

    // The two indexes that are load-bearing rather than merely helpful: without
    // the HNSW index every similarity query is a sequential scan, and without
    // the partial unique index the debounce upsert has nothing to conflict on.
    for index in ["embeddings_1024_vector_idx", "jobs_live_dedupe_idx"] {
        assert_eq!(
            relation(&pool, index).await.as_deref(),
            Some(index),
            "{index} carries a decision, not a micro-optimisation"
        );
    }

    // PRD req 4: the store is Postgres *with pgvector*, and the migration is
    // where a stack missing it is supposed to fail.
    let vector: i64 =
        sqlx::query_scalar("SELECT count(*) FROM pg_extension WHERE extname = 'vector'")
            .fetch_one(&pool)
            .await
            .expect("asking for the extension should succeed");
    assert_eq!(vector, 1, "0001 should have created the vector extension");

    database.destroy(pool).await;
}

/// 0004 replaces 0001's `elements`, so the columns the tree model needs have to
/// actually be there — and 0001's flat-table columns have to actually be gone.
///
/// A DROP-and-recreate that silently kept the old table (because the DROP was
/// conditional on something untrue, say) would pass every version assertion
/// above and fail on the first write.
#[tokio::test]
async fn the_elements_table_is_the_tree_shape_not_the_flat_one() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;

    let columns: Vec<String> = sqlx::query_scalar(
        "SELECT column_name::text FROM information_schema.columns
          WHERE table_schema = 'public' AND table_name = 'elements'
          ORDER BY column_name",
    )
    .fetch_all(&pool)
    .await
    .expect("the column list should be readable");

    for column in [
        "blob_sha",
        "parser_version",
        "parent_id",
        "sibling_order",
        "raw_hash",
        "enrich",
    ] {
        assert!(
            columns.iter().any(|found| found == column),
            "0004 must add {column}; got {columns:?}"
        );
    }
    for gone in ["blob", "qualified_name", "ts_kind", "body", "has_error"] {
        assert!(
            !columns.iter().any(|found| found == gone),
            "{gone} belonged to 0001's flat table and must not survive 0004: {columns:?}"
        );
    }

    database.destroy(pool).await;
}

/// Running migrations again is a no-op — which is what makes "restart the
/// daemon" a safe answer, and what lets boot migrate unconditionally.
#[tokio::test]
async fn migrating_twice_changes_nothing() {
    let database = FreshDatabase::create().await;
    let pool = database.pool().await;

    migrate(&pool).await.expect("the first run should apply");
    let after_first = applied_rows(&pool).await;

    migrate(&pool)
        .await
        .expect("the second run should be a no-op");
    let after_second = applied_rows(&pool).await;

    // Identical `installed_on` is the real proof: not merely "the same set of
    // versions", but "nothing ran again".
    assert_eq!(
        after_first, after_second,
        "the second run must not re-apply, re-time or re-checksum anything"
    );
    assert_eq!(
        after_first.len(),
        embedded_versions().len(),
        "no duplicate rows should appear in the tracking table"
    );

    database.destroy(pool).await;
}
