//! Disposable, migrated Postgres databases for tests and hand-run sandboxes.
//!
//! Every `CREATE DATABASE` and `DROP DATABASE` delegates to `fs3_store`, whose
//! process-wide semaphore protects the shared postmaster from DDL bursts.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use fs3_store::{PgPool, StoreError};

/// Namespace reserved for whole-suite databases minted by `harness checks`.
pub const TEST_DATABASE_PREFIX: &str = "fs3_test_";
const TEST_DATABASE_NAMESPACE: &str = "fs3_";

/// A crashed whole-suite database becomes sweepable after this age.
pub const ORPHAN_SWEEP_AGE: Duration = Duration::from_secs(6 * 60 * 60);

fn store_error_source(error: &StoreError) -> Option<&(dyn std::error::Error + 'static)> {
    match error {
        StoreError::Unreachable { source, .. } | StoreError::Query(source) => Some(source),
        _ => None,
    }
}

fn connection_error_kind(error: &StoreError) -> Option<std::io::ErrorKind> {
    let mut current = store_error_source(error);
    while let Some(source) = current {
        if let Some(error) = source.downcast_ref::<std::io::Error>() {
            return Some(error.kind());
        }
        current = source.source();
    }
    None
}

fn server_rejected_credentials_or_permission(error: &StoreError) -> bool {
    let detail = store_error_source(error)
        .map(ToString::to_string)
        .unwrap_or_else(|| error.to_string())
        .to_ascii_lowercase();
    detail.contains("28p01")
        || detail.contains("invalid_password")
        || detail.contains("authentication failed")
        || detail.contains("password authentication")
        || detail.contains("42501")
        || detail.contains("insufficient_privilege")
        || detail.contains("permission denied")
}

fn server_is_recovering(error: &StoreError) -> bool {
    let detail = store_error_source(error)
        .map(ToString::to_string)
        .unwrap_or_else(|| error.to_string())
        .to_ascii_lowercase();
    detail.contains("57p03")
        || detail.contains("database system is in recovery mode")
        || matches!(
            connection_error_kind(error),
            Some(
                std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::ConnectionAborted
                    | std::io::ErrorKind::BrokenPipe
                    | std::io::ErrorKind::UnexpectedEof
            )
        )
}

fn postgres_endpoint(url: &str) -> Option<(&str, u16)> {
    let authority = url
        .strip_prefix("postgres://")
        .or_else(|| url.strip_prefix("postgresql://"))?
        .split('/')
        .next()?;
    let host_port = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host_port)| host_port);
    if let Some(bracketed) = host_port.strip_prefix('[') {
        let close = bracketed.find(']')?;
        let host = &bracketed[..close];
        let suffix = &bracketed[close + 1..];
        let port = if suffix.is_empty() {
            5432
        } else {
            suffix.strip_prefix(':')?.parse().ok()?
        };
        return (!host.is_empty()).then_some((host, port));
    }

    let (host, port) = match host_port.rsplit_once(':') {
        Some((host, port)) => (host, port.parse().ok()?),
        None => (host_port, 5432),
    };
    (!host.is_empty()).then_some((host, port))
}

async fn postgres_server_is_listening(base_url: &str) -> Option<bool> {
    let endpoint = postgres_endpoint(base_url)?;
    match tokio::time::timeout(
        Duration::from_millis(250),
        tokio::net::TcpStream::connect(endpoint),
    )
    .await
    {
        Ok(Ok(stream)) => {
            drop(stream);
            Some(true)
        }
        Ok(Err(error)) if error.kind() == std::io::ErrorKind::ConnectionRefused => Some(false),
        Ok(Err(_)) | Err(_) => None,
    }
}

fn creation_failure_message(
    base_url: &str,
    error: &StoreError,
    server_listening: Option<bool>,
) -> String {
    if server_listening == Some(false) {
        return format!(
            "No server at {base_url}; start one with:\n    {}\nThen re-run the test.",
            fs3_store::COMPOSE_UP
        );
    }

    let detail = store_error_source(error)
        .map(ToString::to_string)
        .unwrap_or_else(|| error.to_string());
    if server_rejected_credentials_or_permission(error) {
        return format!(
            "Server at {base_url} rejected authentication or database permissions: {detail}\nFix the credentials or permissions before retrying."
        );
    }
    if server_listening == Some(true) || server_is_recovering(error) {
        format!(
            "Server at {base_url} closed the connection or is in recovery; wait and retry.\nCause: {detail}"
        )
    } else {
        format!(
            "Could not create a test database at {base_url}: {detail}\nCheck the address, credentials, and server logs, then retry."
        )
    }
}

/// What an orphan sweep did, including the policy that decided it.
#[derive(Debug, PartialEq, Eq)]
pub struct SweepReport {
    pub threshold: Duration,
    pub swept: Vec<String>,
}

/// Test databases a sweep would remove, without changing the server.
#[derive(Debug, PartialEq, Eq)]
pub struct OrphanCandidates {
    pub threshold: Duration,
    pub candidates: Vec<String>,
}

/// A uniquely named database on an existing Postgres server.
///
/// [`FreshDatabase::create_from`] applies the embedded migrations; the legacy
/// test constructor leaves migration timing to the test that owns it.
/// Cleanup is explicit because `Drop` cannot await. If a process is killed,
/// the unique name remains as the truthful, findable record of what leaked.
pub struct FreshDatabase {
    name: String,
    url: String,
    maintenance_url: String,
}

impl FreshDatabase {
    /// Create an empty database on the test server selected by
    /// `FS3_TEST_DATABASE_URL`.
    ///
    /// Migration remains explicit for compatibility with schema-skew tests
    /// whose subject is the transition from an empty or behind database.
    /// Panics rather than skipping when Postgres is unavailable, matching the
    /// repository's integration-test contract.
    pub async fn create(label: &str) -> Self {
        let base_url = crate::test_database_url();
        Self::create_with_advice(&base_url, label).await
    }

    async fn create_with_advice(base_url: &str, label: &str) -> Self {
        match Self::create_unmigrated_from(base_url, label).await {
            Ok(database) => database,
            Err(error) => {
                let server_listening = postgres_server_is_listening(base_url).await;
                panic!(
                    "{}",
                    creation_failure_message(base_url, &error, server_listening)
                )
            }
        }
    }

    /// Create and migrate a disposable database beside `base_url`.
    ///
    /// The named database in `base_url` is never migrated or otherwise used as
    /// application data; only its server and credentials select where the new
    /// child database is created.
    pub async fn create_from(base_url: &str, label: &str) -> Result<Self, StoreError> {
        let database = Self::create_unmigrated_from(base_url, label).await?;
        let pool = match fs3_store::connect(&database.url).await {
            Ok(pool) => pool,
            Err(error) => {
                let _ = database.cleanup().await;
                return Err(error);
            }
        };
        if let Err(error) = fs3_store::migrate(&pool).await {
            pool.close().await;
            let _ = database.cleanup().await;
            return Err(error);
        }
        pool.close().await;
        Ok(database)
    }

    async fn create_unmigrated_from(base_url: &str, label: &str) -> Result<Self, StoreError> {
        let (maintenance_url, _) = fs3_store::maintenance_url(base_url)?;
        let admin = fs3_store::connect(&maintenance_url).await?;
        let name = database_name_at(label, SystemTime::now());
        if let Err(error) = fs3_store::create_database(&admin, &name).await {
            admin.close().await;
            return Err(error);
        }
        let url = fs3_store::database_url(base_url, &name)?;
        admin.close().await;

        Ok(Self {
            name,
            url,
            maintenance_url,
        })
    }

    /// List crashed test databases old enough to sweep, without dropping them.
    ///
    /// Read-only snap-in: `FreshDatabase::list_orphans_from(base_url).await?`.
    /// Print `report.candidates` for review before calling
    /// [`FreshDatabase::sweep_orphans_from`].
    pub async fn list_orphans_from(base_url: &str) -> Result<OrphanCandidates, StoreError> {
        Self::list_orphans_at(base_url, SystemTime::now(), ORPHAN_SWEEP_AGE).await
    }

    async fn list_orphans_at(
        base_url: &str,
        now: SystemTime,
        threshold: Duration,
    ) -> Result<OrphanCandidates, StoreError> {
        let (maintenance_url, _) = fs3_store::maintenance_url(base_url)?;
        let admin = fs3_store::connect(&maintenance_url).await?;
        let names =
            match fs3_store::idle_database_names_with_prefix(&admin, TEST_DATABASE_NAMESPACE).await
            {
                Ok(names) => names,
                Err(error) => {
                    admin.close().await;
                    return Err(error);
                }
            };
        admin.close().await;

        let cutoff = epoch_seconds(now).saturating_sub(threshold.as_secs());
        let mut candidates: Vec<_> = names
            .into_iter()
            .filter(|name| {
                test_database_created_at(name).is_some_and(|created_at| created_at <= cutoff)
            })
            .collect();
        candidates.sort();
        Ok(OrphanCandidates {
            threshold,
            candidates,
        })
    }

    /// Drop crashed test databases older than [`ORPHAN_SWEEP_AGE`].
    ///
    /// Malformed names and databases younger than the threshold are never
    /// touched. Both whole-suite and label-prefixed names minted by this helper
    /// are eligible.
    pub async fn sweep_orphans_from(base_url: &str) -> Result<SweepReport, StoreError> {
        Self::sweep_orphans_at(base_url, SystemTime::now(), ORPHAN_SWEEP_AGE).await
    }

    async fn sweep_orphans_at(
        base_url: &str,
        now: SystemTime,
        threshold: Duration,
    ) -> Result<SweepReport, StoreError> {
        let candidates = Self::list_orphans_at(base_url, now, threshold).await?;
        let (maintenance_url, _) = fs3_store::maintenance_url(base_url)?;
        let admin = fs3_store::connect(&maintenance_url).await?;
        let report = Self::sweep_candidates(&admin, threshold, candidates.candidates).await;
        admin.close().await;
        report
    }

    async fn sweep_candidates(
        admin: &PgPool,
        threshold: Duration,
        candidates: Vec<String>,
    ) -> Result<SweepReport, StoreError> {
        let mut swept = Vec::with_capacity(candidates.len());
        for name in candidates {
            match fs3_store::drop_database_if_idle(admin, &name).await {
                Ok(true) => swept.push(name),
                Ok(false) => {}
                Err(error) => return Err(error),
            }
        }
        Ok(SweepReport { threshold, swept })
    }

    /// The plain Postgres database name, safe to print in a leftover warning.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The connection URL for this database.
    #[must_use]
    pub fn url(&self) -> String {
        self.url.clone()
    }

    /// Open a pool onto the database.
    pub async fn pool(&self) -> PgPool {
        fs3_store::connect(&self.url)
            .await
            .unwrap_or_else(|error| panic!("connecting to {}: {error}", self.name))
    }

    /// Close a known application pool, then remove the database.
    pub async fn destroy(self, pool: PgPool) {
        pool.close().await;
        self.destroy_force().await;
    }

    /// Remove the database, forcing closed any remaining application sessions.
    pub async fn cleanup(self) -> Result<(), StoreError> {
        // Reconnect rather than retaining the creation pool. A sandbox stops
        // its worker runtime before cleanup, and sqlx pools are runtime-bound.
        let admin = fs3_store::connect(&self.maintenance_url).await?;
        let result = fs3_store::drop_database(&admin, &self.name).await;
        admin.close().await;
        result
    }

    /// Test-friendly cleanup that panics rather than silently leaking.
    pub async fn destroy_force(self) {
        self.cleanup()
            .await
            .unwrap_or_else(|error| panic!("dropping database: {error}"));
    }
}

fn database_name_at(label: &str, created_at: SystemTime) -> String {
    let label: String = label
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .take(12)
        .collect();
    format!(
        "fs3_{label}_{}_{:032x}",
        epoch_seconds(created_at),
        unique_seed()
    )
}

fn test_database_created_at(name: &str) -> Option<u64> {
    let tail = name.strip_prefix(TEST_DATABASE_NAMESPACE)?;
    let (label_and_time, entropy) = tail.rsplit_once('_')?;
    let (label, created_at) = label_and_time.rsplit_once('_')?;
    if label.len() > 12
        || !label.bytes().all(|byte| byte.is_ascii_alphanumeric())
        || entropy.len() != 32
        || !entropy.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return None;
    }
    created_at.parse().ok()
}

fn epoch_seconds(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .expect("the clock is after 1970")
        .as_secs()
}

fn unique_seed() -> u128 {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("the clock is after 1970")
        .as_nanos();
    nanos
        ^ (u128::from(std::process::id()) << 64)
        ^ (u128::from(SEQUENCE.fetch_add(1, Ordering::Relaxed)) << 96)
}

#[cfg(test)]
mod tests {
    use std::{
        any::Any,
        time::{Duration, SystemTime},
    };

    use super::{FreshDatabase, ORPHAN_SWEEP_AGE, database_name_at, test_database_created_at};

    fn panic_text(panic: Box<dyn Any + Send>) -> String {
        match panic.downcast::<String>() {
            Ok(message) => *message,
            Err(panic) => match panic.downcast::<&'static str>() {
                Ok(message) => (*message).to_owned(),
                Err(_) => "non-string panic".to_owned(),
            },
        }
    }

    #[tokio::test]
    async fn advice_says_to_start_a_server_only_when_no_server_is_listening() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        let base_url = format!("postgres://flowspace3:flowspace3@{address}/postgres");

        let task =
            tokio::spawn(
                async move { FreshDatabase::create_with_advice(&base_url, "advice").await },
            );
        let error = match task.await {
            Ok(_) => panic!("database creation unexpectedly succeeded"),
            Err(error) => error,
        };
        let message = panic_text(error.into_panic());

        assert!(message.contains("No server at"), "{message}");
        assert!(message.contains(fs3_store::COMPOSE_UP), "{message}");
    }

    #[tokio::test]
    async fn advice_says_fix_credentials_when_authentication_is_rejected() {
        let base_url = "postgres://flowspace3:wrong-password@127.0.0.1:5433/postgres".to_owned();
        let task =
            tokio::spawn(
                async move { FreshDatabase::create_with_advice(&base_url, "advice").await },
            );
        let error = match task.await {
            Ok(_) => panic!("database creation unexpectedly succeeded"),
            Err(error) => error,
        };
        let message = panic_text(error.into_panic());

        assert!(message.contains("rejected authentication"), "{message}");
        assert!(message.contains("Fix the credentials"), "{message}");
        assert!(!message.contains("wait and retry"), "{message}");
        assert!(!message.contains(fs3_store::COMPOSE_UP), "{message}");
    }

    #[tokio::test]
    async fn advice_says_wait_when_a_listening_server_closes_connections() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                drop(stream);
            }
        });
        let base_url = format!("postgres://flowspace3:flowspace3@{address}/postgres");

        let task =
            tokio::spawn(
                async move { FreshDatabase::create_with_advice(&base_url, "advice").await },
            );
        let error = match task.await {
            Ok(_) => panic!("database creation unexpectedly succeeded"),
            Err(error) => error,
        };
        let message = panic_text(error.into_panic());
        server.abort();

        assert!(
            message.to_ascii_lowercase().contains("recover"),
            "{message}"
        );
        assert!(message.contains("wait and retry"), "{message}");
        assert!(!message.contains(fs3_store::COMPOSE_UP), "{message}");
    }

    #[test]
    fn names_are_plain_bounded_unique_and_timestamped() {
        let created_at = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        let first = database_name_at("test", created_at);
        let second = database_name_at("sandbox-with punctuation", SystemTime::now());
        assert_ne!(first, second);
        assert!(first.len() <= 63, "{first}");
        assert!(
            first
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_'),
            "{first}"
        );
        assert_eq!(test_database_created_at(&first), Some(1_700_000_000));
        assert!(second.starts_with("fs3_sandboxwithp_"), "{second}");
    }

    #[test]
    fn malformed_and_production_database_names_are_not_sweepable() {
        for name in [
            "flowspace3",
            "flowspace3_test",
            "fs3_test_1700000000",
            "fs3_test_notatime_0123456789abcdef0123456789abcdef",
            "fs3_test_1700000000_nothex",
            "fs3_label-too-long_1700000000_0123456789abcdef0123456789abcdef",
        ] {
            assert_eq!(test_database_created_at(name), None, "{name}");
        }
        assert_eq!(
            test_database_created_at("fs3_sandbox_1700000000_0123456789abcdef0123456789abcdef"),
            Some(1_700_000_000)
        );
    }

    #[tokio::test]
    async fn sweep_preserves_live_and_post_listing_race_databases() {
        let base_url = crate::test_database_url();
        let (maintenance_url, _) = fs3_store::maintenance_url(&base_url).unwrap();
        let admin = fs3_store::connect(&maintenance_url).await.unwrap();
        let old_time = SystemTime::UNIX_EPOCH + Duration::from_secs(1);
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(ORPHAN_SWEEP_AGE.as_secs() + 2);
        let old_suite = database_name_at("test", old_time);
        let old_label = database_name_at("sweep012", old_time);
        let live = database_name_at("sweeplive", old_time);
        let racing = database_name_at("sweeprace", old_time);
        let fresh = database_name_at("sweep012", now);
        for name in [&old_suite, &old_label, &live, &racing, &fresh] {
            fs3_store::create_database(&admin, name).await.unwrap();
        }
        let live_url = fs3_store::database_url(&base_url, &live).unwrap();
        let live_pool = fs3_store::connect(&live_url).await.unwrap();

        let mut listed = vec![old_suite.clone(), old_label.clone(), racing.clone()];
        listed.sort();
        let candidates = FreshDatabase::list_orphans_at(&base_url, now, ORPHAN_SWEEP_AGE)
            .await
            .unwrap();
        assert_eq!(candidates.threshold, ORPHAN_SWEEP_AGE);
        assert_eq!(candidates.candidates, listed);

        let racing_url = fs3_store::database_url(&base_url, &racing).unwrap();
        let racing_pool = fs3_store::connect(&racing_url).await.unwrap();
        let report = tokio::time::timeout(
            Duration::from_secs(10),
            FreshDatabase::sweep_candidates(&admin, ORPHAN_SWEEP_AGE, candidates.candidates),
        )
        .await
        .expect("the sweep loop must not self-deadlock")
        .unwrap();

        let old_suite_exists = fs3_store::database_exists(&admin, &old_suite)
            .await
            .unwrap();
        let old_label_exists = fs3_store::database_exists(&admin, &old_label)
            .await
            .unwrap();
        let live_exists = fs3_store::database_exists(&admin, &live).await.unwrap();
        let racing_exists = fs3_store::database_exists(&admin, &racing).await.unwrap();
        let fresh_exists = fs3_store::database_exists(&admin, &fresh).await.unwrap();
        let racing_connection_usable = racing_pool.acquire().await.is_ok();

        live_pool.close().await;
        racing_pool.close().await;
        fs3_store::drop_database(&admin, &live).await.unwrap();
        fs3_store::drop_database(&admin, &racing).await.unwrap();
        fs3_store::drop_database(&admin, &fresh).await.unwrap();
        admin.close().await;

        let mut swept = vec![old_suite, old_label];
        swept.sort();
        assert_eq!(report.threshold, ORPHAN_SWEEP_AGE);
        assert_eq!(report.swept, swept);
        assert!(!old_suite_exists);
        assert!(!old_label_exists);
        assert!(live_exists);
        assert!(racing_exists);
        assert!(fresh_exists);
        assert!(racing_connection_usable);
    }
}
