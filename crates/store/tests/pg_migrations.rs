//! What the migration story has to be true for: a fresh database becomes a
//! working store, and running it again changes nothing.
//!
//! `pg_round_trip.rs` migrates the *shared* 5433 database, which by the second
//! run is never fresh — so it can only ever prove the no-op half by accident.
//! These tests create a throwaway database per test and prove both halves on
//! purpose. Like every store test, they need the dockerized Postgres and fail
//! naming the command rather than skipping.

use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use fs3_store::{MIGRATOR, PgPool, migrate};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};

/// Override with `FS3_TEST_DATABASE_URL` to point at another instance.
fn database_url() -> String {
    std::env::var("FS3_TEST_DATABASE_URL")
        .unwrap_or_else(|_| fs3_core::DatabaseConfig::DEFAULT_URL.to_string())
}

/// A throwaway database, created empty and dropped again.
///
/// The name is seeded from the clock, the pid and a per-process counter for the
/// same reason `pg_round_trip` seeds its blob keys that way: these tests DROP
/// what they name, and concurrent runs share one 5433 stack. A collision would
/// have to happen in the same process in the same nanosecond.
struct FreshDatabase {
    name: String,
    admin: PgPool,
}

impl FreshDatabase {
    async fn create() -> Self {
        let url = database_url();
        let admin = PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(fs3_store::CONNECT_TIMEOUT)
            .connect(&url)
            .await
            .unwrap_or_else(|error| {
                panic!(
                    "store migration tests need Postgres at {url}: {error}\nStart it with:\n    \
                     {}\nThen re-run:\n    cargo test -p fs3-store\nPoint at another instance \
                     with FS3_TEST_DATABASE_URL.",
                    fs3_store::COMPOSE_UP
                )
            });

        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("the clock is after 1970")
            .as_nanos();
        let seed = nanos
            ^ (u128::from(std::process::id()) << 64)
            ^ (u128::from(SEQUENCE.fetch_add(1, Ordering::Relaxed)) << 96);
        // Hex from a u128, so the identifier is `fs3_migrations_` + 32 safe
        // characters — well inside Postgres' 63-byte limit, and nothing here can
        // be quoted out of. `CREATE DATABASE` takes no bind parameters.
        let name = format!("fs3_migrations_{seed:032x}");

        sqlx::query(&format!("CREATE DATABASE {name}"))
            .execute(&admin)
            .await
            .unwrap_or_else(|error| panic!("creating the throwaway database {name}: {error}"));

        Self { name, admin }
    }

    /// A pool onto the throwaway database.
    async fn pool(&self) -> PgPool {
        let options = PgConnectOptions::from_str(&database_url())
            .expect("the configured database URL should parse")
            .database(&self.name);
        PgPoolOptions::new()
            .max_connections(2)
            .acquire_timeout(fs3_store::CONNECT_TIMEOUT)
            .connect_with(options)
            .await
            .unwrap_or_else(|error| panic!("connecting to {}: {error}", self.name))
    }

    /// Explicit, because `Drop` cannot await. A test that panics before this
    /// leaves one empty `fs3_migrations_*` database behind — visible, harmless,
    /// and a truthful record that the run failed.
    async fn destroy(self, pool: PgPool) {
        pool.close().await;
        sqlx::query(&format!(
            "DROP DATABASE IF EXISTS {} WITH (FORCE)",
            self.name
        ))
        .execute(&self.admin)
        .await
        .unwrap_or_else(|error| panic!("dropping {}: {error}", self.name));
        self.admin.close().await;
    }
}

/// Every version the binary carries, in order. Asserting against the embedded
/// set rather than a hardcoded `[1]` keeps this test honest when `0002` lands.
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

    let elements: Option<String> =
        sqlx::query_scalar("SELECT to_regclass('public.elements')::text")
            .fetch_one(&pool)
            .await
            .expect("asking for the table should succeed");
    assert_eq!(
        elements.as_deref(),
        Some("elements"),
        "0001 should have created the elements table"
    );

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
