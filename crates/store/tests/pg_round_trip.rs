//! Exemplar: the integration tier.
//!
//! Runs against the real dockerized Postgres from `docker-compose.yml` — there
//! is no in-memory store to run against instead, and that is deliberate
//! (workshop 001 refuses a repository trait over sqlx).
//!
//! If docker is not running this test FAILS rather than skipping, and names the
//! exact command. A silently-skipped integration test is how a store regression
//! reaches main.

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use fs3_core::{BlobRef, Element, ElementKind};
use fs3_store::{StoreError, connect, elements_for_blob, migrate, upsert_element};
use sqlx::PgPool;

/// Override with `FS3_TEST_DATABASE_URL` to point at another instance.
fn database_url() -> String {
    std::env::var("FS3_TEST_DATABASE_URL")
        .unwrap_or_else(|_| fs3_core::DatabaseConfig::DEFAULT_URL.to_string())
}

/// A blob key nobody else is using.
///
/// Keying on `process::id()` alone was not isolation. These tests DELETE by
/// blob, so a collision against the shared 5433 stack does not merely
/// interfere — it cross-deletes another run's rows, and pids are recycled
/// freely across concurrent runs and separate checkouts. Seeding from the
/// clock, the pid and a per-process counter makes a collision require the same
/// process id in the same nanosecond.
fn unique_blob() -> BlobRef {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("the clock is after 1970")
        .as_nanos();
    let seed = nanos
        ^ (u128::from(std::process::id()) << 64)
        ^ (u128::from(SEQUENCE.fetch_add(1, Ordering::Relaxed)) << 96);
    // A u128 is at most 32 hex digits, so this is always a 40-char digest.
    BlobRef::new(format!("{seed:040x}")).expect("40 hex digits is a valid digest")
}

async fn ready_pool() -> PgPool {
    let url = database_url();
    // The error already names `docker compose up -d`; this adds only what the
    // error cannot know — which test to re-run, and the escape hatch.
    let pool = connect(&url).await.unwrap_or_else(|error| {
        panic!(
            "store integration test needs Postgres.\n{error}\nThen re-run:\n    \
             cargo test -p fs3-store\nPoint at another instance with \
             FS3_TEST_DATABASE_URL."
        )
    });
    migrate(&pool).await.expect("migration 0001 should apply");
    pool
}

fn element(blob: &BlobRef, qualified_name: &str, start: u32, end: u32) -> Element {
    Element {
        path: "parsers/fixtures/sample.rs".to_string(),
        blob: blob.clone(),
        ts_kind: "function_item".to_string(),
        kind: ElementKind::Callable,
        qualified_name: qualified_name.to_string(),
        start_line: start,
        end_line: end,
        text: format!("pub fn {qualified_name}() {{}}"),
        has_error: false,
    }
}

/// Connect, migrate, round-trip. The whole exemplar in one test.
#[tokio::test]
async fn migration_applies_and_an_element_round_trips() {
    let pool = ready_pool().await;

    // A unique blob per run keeps repeated and CONCURRENT runs independent
    // without truncating a table another run may be using.
    let blob = unique_blob();
    sqlx::query("DELETE FROM elements WHERE blob = $1")
        .bind(blob.as_str())
        .execute(&pool)
        .await
        .expect("cleanup should succeed");

    let written = element(&blob, "geometry.Rect.new", 11, 16);
    upsert_element(&pool, &written).await.expect("insert");

    let read_back = elements_for_blob(&pool, &blob).await.expect("select");
    assert_eq!(
        read_back,
        vec![written.clone()],
        "the row must survive the trip"
    );

    // Upsert is keyed by the content address, so re-indexing is idempotent.
    upsert_element(&pool, &written).await.expect("re-insert");
    assert_eq!(elements_for_blob(&pool, &blob).await.unwrap().len(), 1);

    sqlx::query("DELETE FROM elements WHERE blob = $1")
        .bind(blob.as_str())
        .execute(&pool)
        .await
        .expect("cleanup should succeed");
}

/// pgvector is a requirement of the image, not an optional extra (PRD req 4).
#[tokio::test]
async fn the_stack_really_has_pgvector() {
    let pool = ready_pool().await;
    let installed: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'vector')")
            .fetch_one(&pool)
            .await
            .expect("extension probe should run");
    assert!(
        installed,
        "migration 0001 creates the vector extension; the image must provide it \
         (docker-compose.yml pins pgvector/pgvector)"
    );
}

/// The CHECK constraints are part of the schema's contract, so prove they bite.
#[tokio::test]
async fn the_schema_refuses_an_impossible_span() {
    let pool = ready_pool().await;
    let blob = unique_blob();

    let mut backwards = element(&blob, "backwards", 40, 10);
    backwards.end_line = 10;
    let error = upsert_element(&pool, &backwards)
        .await
        .expect_err("end_line < start_line must be refused by the database, not just by Rust");

    // `is_err()` alone passed for ANY failure — a dropped connection, a typo in
    // the SQL, a missing table. What this test claims is that the CHECK bit, so
    // it has to read the SQLSTATE the database sent.
    let StoreError::Query(sqlx::Error::Database(database_error)) = &error else {
        panic!("expected a database error carrying a SQLSTATE, got: {error}");
    };
    assert_eq!(
        database_error.code().as_deref(),
        Some("23514"),
        "23514 is check_violation; another code means the row was refused for \
         the wrong reason: {error}"
    );
    assert_eq!(
        database_error.constraint(),
        Some("elements_span_ordered"),
        "the span CHECK must be the constraint that bit, not another one: {error}"
    );
}

/// The isolation key has to vary WITHIN a process too: these tests run
/// concurrently under one `cargo test` binary, against one database.
#[test]
fn every_blob_key_is_unique_within_a_process() {
    let keys: BTreeSet<String> = (0..1000)
        .map(|_| unique_blob().as_str().to_string())
        .collect();
    assert_eq!(
        keys.len(),
        1000,
        "a key derived from the pid alone repeats on every call, which is how \
         two tests end up deleting each other's rows"
    );

    let key = unique_blob();
    assert_eq!(key.as_str().len(), 40, "a blob key is a 40-char digest");
    assert!(
        key.as_str()
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
        "a blob key is lowercase hex: {}",
        key.as_str()
    );
}
