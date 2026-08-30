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

    // These indexes carry query shapes, not micro-optimisations: removing one
    // restores a whole-table scan for its retrieval leg.
    for index in [
        "embeddings_1024_vector_idx",
        "elements_lexical_trgm_idx",
        "jobs_live_dedupe_idx",
        "jobs_claim_embed_idx",
        "jobs_claim_general_idx",
    ] {
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

    let trigram: i64 =
        sqlx::query_scalar("SELECT count(*) FROM pg_extension WHERE extname = 'pg_trgm'")
            .fetch_one(&pool)
            .await
            .expect("asking for the extension should succeed");
    assert_eq!(trigram, 1, "0018 should have created the trigram extension");
    let definitions: Vec<(String, String)> = sqlx::query_as(
        "SELECT indexname, indexdef
           FROM pg_indexes
          WHERE schemaname = 'public'
            AND indexname IN ('jobs_claim_embed_idx', 'jobs_claim_general_idx')
          ORDER BY indexname",
    )
    .fetch_all(&pool)
    .await
    .expect("the pending claim indexes should be inspectable");
    assert_eq!(definitions.len(), 2, "both claim lanes need an access path");
    for (name, definition) in definitions {
        assert!(
            definition.contains("(priority DESC, id DESC) INCLUDE (not_before)"),
            "{name} must retain priority-first LIFO ordering: {definition}"
        );
        assert!(
            definition.contains("state = 'pending'"),
            "{name} must exclude settled history: {definition}"
        );
        match name.as_str() {
            "jobs_claim_embed_idx" => assert!(definition.contains("kind = 'embed'")),
            "jobs_claim_general_idx" => {
                assert!(definition.contains("scan_file") && definition.contains("summarize"));
            }
            _ => unreachable!("the query admits only the two claim indexes"),
        }
    }

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

/// Apply the embedded migrations in `versions`, in order, without touching
/// `_sqlx_migrations`.
///
/// The point is to stand a database up in a genuinely PRE-migration state so
/// the next migration can be run AT it. Nothing here reimplements a migration:
/// the SQL is the embedded text, so a migration edited tomorrow is the SQL this
/// runs tomorrow. A RANGE rather than a ceiling because these are not
/// idempotent — 0004 replaces 0001's `elements`, so re-running from the start
/// fails on a schema the earlier migration no longer describes.
async fn apply_migrations(pool: &PgPool, versions: std::ops::RangeInclusive<i64>) {
    for migration in MIGRATOR
        .iter()
        .filter(|entry| versions.contains(&entry.version))
    {
        sqlx::raw_sql(&migration.sql)
            .execute(pool)
            .await
            .unwrap_or_else(|error| {
                panic!("migration {} should apply: {error}", migration.version)
            });
    }
}

/// The migration that had to double as a RECOVERY (Jordan, 2026-08-27).
///
/// A daemon run from a throwaway dev worktree wrote `update:blocked` naming its
/// own `target/debug` path into a shared store. The production daemon restarted
/// the next day on a current binary and went on serving that fossil to every
/// envelope, because update state was keyed to the STORE and there was nothing
/// to re-evaluate it against.
///
/// The fix arrives as a new binary, and a new binary migrates at boot — so the
/// migration is where the fossil dies. No repair verb, no hand-written SQL for
/// a user to find in a doc. What it must NOT do is take the other producers'
/// messages with it: a schema skew is exactly as true after this statement as
/// before it.
#[tokio::test]
async fn migrating_to_per_install_update_state_kills_the_store_keyed_fossils() {
    let database = FreshDatabase::create().await;
    let pool = database.pool().await;

    apply_migrations(&pool, 1..=11).await;

    // The world as a pre-fix binary left it: one singleton row describing a
    // path that only ever existed in somebody's build tree, and the standing
    // message it produced.
    sqlx::query(
        "UPDATE update_state
            SET installed_version = '0.2.0',
                install_path      = '/tmp/worktree/target/debug/flowspace3',
                blocked_reason    = 'the install directory is not writable'
          WHERE singleton",
    )
    .execute(&pool)
    .await
    .expect("seeding the pre-fix update row");

    for (key, source) in [
        ("update:blocked", "update"),
        ("update:installed:0.2.0", "update"),
        ("schema:ahead:9001", "schema"),
    ] {
        sqlx::query(
            "INSERT INTO user_messages (key, source, severity, text, next_action)
                  VALUES ($1, $2, 'warning', 'seeded', 'do the thing')",
        )
        .bind(key)
        .bind(source)
        .execute(&pool)
        .await
        .expect("seeding a pre-fix message");
    }

    apply_migrations(&pool, 12..=12).await;

    let survivors: Vec<(String, String)> =
        sqlx::query_as("SELECT key, source FROM user_messages ORDER BY key")
            .fetch_all(&pool)
            .await
            .expect("reading the queue back");
    assert_eq!(
        survivors,
        vec![("schema:ahead:9001".to_string(), "schema".to_string())],
        "every store-keyed update message must die, and nobody else's may"
    );

    // The state row goes with them: it is keyed to a store, so there is no
    // honest install to re-home it onto, and one check re-derives all of it.
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM update_state")
        .fetch_one(&pool)
        .await
        .expect("counting update rows");
    assert_eq!(rows, 0, "the singleton row must not survive its own schema");

    // And the shape it left behind is the per-install one.
    let key_column: String = sqlx::query_scalar(
        "SELECT a.attname
           FROM pg_index i
           JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = ANY(i.indkey)
          WHERE i.indrelid = 'update_state'::regclass AND i.indisprimary",
    )
    .fetch_one(&pool)
    .await
    .expect("reading the primary key");
    assert_eq!(key_column, "install_path");

    database.destroy(pool).await;
}
