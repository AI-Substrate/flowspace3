//! Shared scaffolding for the daemon's integration tests.
//!
//! Every helper here is compiled into each test binary and no binary uses all
//! of it, which is what the allow is for.
#![allow(dead_code)]

use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::Router;
use fs3_store::PgPool;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};

/// Bind `127.0.0.1:0`, serve `router` on a background task, return its base URL.
pub async fn spawn(router: Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("an ephemeral port should be available");
    let address = listener.local_addr().expect("the socket is bound");

    tokio::spawn(async move {
        axum::serve(listener, router).await.expect("server runs");
    });

    format!("http://{address}")
}

/// A fresh directory under the system temp dir. Unique per call, so tests that
/// run in parallel never share one.
pub fn temp_dir(label: &str) -> std::path::PathBuf {
    use std::sync::atomic::AtomicU32;
    static COUNTER: AtomicU32 = AtomicU32::new(0);

    let path = std::env::temp_dir().join(format!(
        "fs3-{label}-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&path).expect("creating a temp directory");
    path
}

/// Override with `FS3_TEST_DATABASE_URL` to point at another instance.
pub fn database_url() -> String {
    std::env::var("FS3_TEST_DATABASE_URL")
        .unwrap_or_else(|_| fs3_core::DatabaseConfig::DEFAULT_URL.to_string())
}

/// The same server, a different database name.
pub fn database_url_named(name: &str) -> String {
    let (maintenance, _) =
        fs3_store::maintenance_url(&database_url()).expect("the configured URL parses");
    maintenance.replace("/postgres", &format!("/{name}"))
}

/// A value nobody else in this process, or any concurrent run, is using.
///
/// Seeded from the clock, the pid and a per-process counter: these tests DROP
/// what they name against a shared stack, and pids are recycled freely across
/// concurrent runs and separate checkouts.
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

/// A throwaway database, created empty and dropped again.
///
/// The daemon's tests need this even more than the store's: `claim_job` takes
/// the best ready job in the WHOLE table, so a concurrent test's pending row
/// would not merely coexist — the runner would claim it and run it.
pub struct FreshDatabase {
    name: String,
    admin: PgPool,
}

impl FreshDatabase {
    /// Create the database. Fails naming the command rather than skipping — a
    /// silently-skipped integration test is how a regression reaches main.
    pub async fn create(label: &str) -> Self {
        let url = database_url();
        let admin = PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(fs3_store::CONNECT_TIMEOUT)
            .connect(&url)
            .await
            .unwrap_or_else(|error| {
                panic!(
                    "daemon integration tests need Postgres at {url}: {error}\nStart it with:\n \
                     {}\nThen re-run:\n    cargo test -p fs3-daemon\nPoint at another instance \
                     with FS3_TEST_DATABASE_URL.",
                    fs3_store::COMPOSE_UP
                )
            });

        // Hex from a u128, so the identifier is safe by construction —
        // `CREATE DATABASE` takes no bind parameters.
        let label: String = label
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .take(12)
            .collect();
        let name = format!("fs3_daemon_{label}_{:032x}", unique_seed());

        sqlx::query(&format!("CREATE DATABASE {name}"))
            .execute(&admin)
            .await
            .unwrap_or_else(|error| panic!("creating the throwaway database {name}: {error}"));

        Self { name, admin }
    }

    /// The URL of this throwaway database.
    pub fn url(&self) -> String {
        database_url_named(&self.name)
    }

    /// A pool onto it.
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

    /// Explicit, because `Drop` cannot await. A test that panics before this
    /// leaves one database behind — visible, harmless, and a truthful record
    /// that the run failed.
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

/// Drop a database by name — for tests that created one without a
/// [`FreshDatabase`] handle.
pub async fn drop_database(name: &str) {
    let admin = PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(fs3_store::CONNECT_TIMEOUT)
        .connect(&database_url())
        .await
        .expect("the shared stack must be up");
    sqlx::query(&format!("DROP DATABASE IF EXISTS {name} WITH (FORCE)"))
        .execute(&admin)
        .await
        .unwrap_or_else(|error| panic!("dropping {name}: {error}"));
    admin.close().await;
}
