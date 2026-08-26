//! Exemplar: the integration tier.
//!
//! Runs against the real dockerized Postgres from `docker-compose.yml` — there
//! is no in-memory store to run against instead, and that is deliberate
//! (workshop 001 refuses a repository trait over sqlx).
//!
//! If docker is not running this test FAILS rather than skipping, and names the
//! exact command. A silently-skipped integration test is how a store regression
//! reaches main.

use fs3_core::{BlobRef, Element, ElementKind};
use fs3_store::{connect, elements_for_blob, migrate, upsert_element};
use sqlx::PgPool;

/// Override with `FS3_TEST_DATABASE_URL` to point at another instance.
fn database_url() -> String {
    std::env::var("FS3_TEST_DATABASE_URL")
        .unwrap_or_else(|_| fs3_core::DatabaseConfig::DEFAULT_URL.to_string())
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

    // A unique blob per run keeps repeated runs independent without truncating
    // a table other tests may be using.
    let blob = BlobRef::new(format!("{:040x}", std::process::id())).unwrap();
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
    let blob = BlobRef::new(format!("{:040x}", std::process::id() + 1)).unwrap();

    let mut backwards = element(&blob, "backwards", 40, 10);
    backwards.end_line = 10;
    let result = upsert_element(&pool, &backwards).await;

    assert!(
        result.is_err(),
        "end_line < start_line must be refused by the database, not just by Rust"
    );
}
