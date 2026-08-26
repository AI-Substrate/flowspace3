//! Shared scaffolding for the store's integration tests.
//!
//! `FreshDatabase` used to live inside `pg_migrations.rs`. It moved here the
//! moment a second test file needed the same isolation: two copies of a helper
//! that DROPs databases is two chances to fix a bug in only one of them.
//!
//! Every test binary compiles this module in full, and no single binary uses
//! all of it — which is what the allow is for. The alternative is one feature
//! gate per helper, which would be more machinery than the thing it guards.
#![allow(dead_code)]

use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use fs3_core::BlobRef;
use fs3_store::PgPool;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};

/// The parser identity the tests write under. A literal, because these tests
/// are about the store, and pinning it makes "same key" and "different key"
/// deliberate choices rather than incidental ones.
pub const PARSER_VERSION: &str = "test-parser@1";

/// Override with `FS3_TEST_DATABASE_URL` to point at another instance.
pub fn database_url() -> String {
    std::env::var("FS3_TEST_DATABASE_URL")
        .unwrap_or_else(|_| fs3_core::DatabaseConfig::DEFAULT_URL.to_string())
}

/// A value nobody else in this process, or any concurrent run, is using.
///
/// Seeded from the clock, the pid and a per-process counter: these tests DROP
/// and DELETE what they name against a shared 5433 stack, and pids are recycled
/// freely across concurrent runs and separate checkouts. A collision would have
/// to happen in the same process in the same nanosecond.
pub fn unique_seed() -> u128 {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("the clock is after 1970")
        .as_nanos();
    nanos
        ^ (u128::from(std::process::id()) << 64)
        ^ (u128::from(SEQUENCE.fetch_add(1, Ordering::Relaxed)) << 96)
}

/// A blob key nobody else is using.
pub fn unique_blob() -> BlobRef {
    // A u128 is at most 32 hex digits, so this is always a 40-char digest.
    BlobRef::new(format!("{:040x}", unique_seed())).expect("40 hex digits is a valid digest")
}

/// A throwaway database, created empty and dropped again.
///
/// Stronger isolation than a unique key in the shared database, and the queue
/// tests need it: `claim_job` takes the best ready job in the WHOLE table, so a
/// concurrent test's pending row would be a real, silent interference.
pub struct FreshDatabase {
    name: String,
    admin: PgPool,
}

impl FreshDatabase {
    /// Create the database. Fails naming the command rather than skipping — a
    /// silently-skipped integration test is how a store regression reaches main.
    pub async fn create() -> Self {
        let url = database_url();
        let admin = PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(fs3_store::CONNECT_TIMEOUT)
            .connect(&url)
            .await
            .unwrap_or_else(|error| {
                panic!(
                    "store integration tests need Postgres at {url}: {error}\nStart it with:\n    \
                     {}\nThen re-run:\n    cargo test -p fs3-store\nPoint at another instance \
                     with FS3_TEST_DATABASE_URL.",
                    fs3_store::COMPOSE_UP
                )
            });

        // Hex from a u128, so the identifier is `fs3_migrations_` + 32 safe
        // characters — well inside Postgres' 63-byte limit, and nothing here can
        // be quoted out of. `CREATE DATABASE` takes no bind parameters.
        let name = format!("fs3_migrations_{:032x}", unique_seed());

        sqlx::query(&format!("CREATE DATABASE {name}"))
            .execute(&admin)
            .await
            .unwrap_or_else(|error| panic!("creating the throwaway database {name}: {error}"));

        Self { name, admin }
    }

    /// A pool onto the throwaway database.
    ///
    /// More than one connection on purpose: the claim tests hold a row lock on
    /// one connection while claiming from another, which is the only way to
    /// prove `SKIP LOCKED` rather than assume it.
    pub async fn pool(&self) -> PgPool {
        let options = PgConnectOptions::from_str(&database_url())
            .expect("the configured database URL should parse")
            .database(&self.name);
        PgPoolOptions::new()
            .max_connections(4)
            .acquire_timeout(fs3_store::CONNECT_TIMEOUT)
            .connect_with(options)
            .await
            .unwrap_or_else(|error| panic!("connecting to {}: {error}", self.name))
    }

    /// A migrated pool — the usual starting point.
    pub async fn migrated_pool(&self) -> PgPool {
        let pool = self.pool().await;
        fs3_store::migrate(&pool)
            .await
            .expect("a fresh database should migrate");
        pool
    }

    /// Explicit, because `Drop` cannot await. A test that panics before this
    /// leaves one empty `fs3_migrations_*` database behind — visible, harmless,
    /// and a truthful record that the run failed.
    pub async fn destroy(self, pool: PgPool) {
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
