//! Disposable, migrated Postgres databases for tests and hand-run sandboxes.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use fs3_store::{PgPool, StoreError};

/// Namespace reserved for whole-suite databases minted by `harness checks`.
pub const TEST_DATABASE_PREFIX: &str = "fs3_test_";

/// A crashed whole-suite database becomes sweepable after this age.
pub const ORPHAN_SWEEP_AGE: Duration = Duration::from_secs(6 * 60 * 60);

/// What an orphan sweep did, including the policy that decided it.
#[derive(Debug, PartialEq, Eq)]
pub struct SweepReport {
    pub threshold: Duration,
    pub swept: Vec<String>,
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
        Self::create_unmigrated_from(&base_url, label)
            .await
            .unwrap_or_else(|error| {
                panic!(
                    "integration tests need Postgres at {base_url}: {error}\nStart it with:\n    {}\n\
                     Then re-run the test. Point at another disposable instance with \
                     FS3_TEST_DATABASE_URL.",
                    fs3_store::COMPOSE_UP
                )
            })
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

    /// Drop crashed whole-suite databases older than [`ORPHAN_SWEEP_AGE`].
    ///
    /// Names outside [`TEST_DATABASE_PREFIX`], malformed names, and databases
    /// younger than the threshold are never touched.
    pub async fn sweep_orphans_from(base_url: &str) -> Result<SweepReport, StoreError> {
        Self::sweep_orphans_at(base_url, SystemTime::now(), ORPHAN_SWEEP_AGE).await
    }

    async fn sweep_orphans_at(
        base_url: &str,
        now: SystemTime,
        threshold: Duration,
    ) -> Result<SweepReport, StoreError> {
        let (maintenance_url, _) = fs3_store::maintenance_url(base_url)?;
        let admin = fs3_store::connect(&maintenance_url).await?;
        let names = match fs3_store::database_names_with_prefix(&admin, TEST_DATABASE_PREFIX).await
        {
            Ok(names) => names,
            Err(error) => {
                admin.close().await;
                return Err(error);
            }
        };
        let cutoff = epoch_seconds(now).saturating_sub(threshold.as_secs());
        let mut swept = Vec::new();
        for name in names {
            if test_database_created_at(&name).is_some_and(|created_at| created_at <= cutoff) {
                if let Err(error) = fs3_store::drop_database(&admin, &name).await {
                    admin.close().await;
                    return Err(error);
                }
                swept.push(name);
            }
        }
        admin.close().await;
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
    let (created_at, entropy) = name.strip_prefix(TEST_DATABASE_PREFIX)?.split_once('_')?;
    if entropy.len() != 32 || !entropy.bytes().all(|byte| byte.is_ascii_hexdigit()) {
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
    use std::time::{Duration, SystemTime};

    use super::{
        FreshDatabase, ORPHAN_SWEEP_AGE, TEST_DATABASE_PREFIX, database_name_at, epoch_seconds,
        test_database_created_at, unique_seed,
    };

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
    fn malformed_test_database_names_are_not_sweepable() {
        for name in [
            "fs3_test_1700000000",
            "fs3_test_notatime_0123456789abcdef0123456789abcdef",
            "fs3_test_1700000000_nothex",
            "fs3_sandbox_1700000000_0123456789abcdef0123456789abcdef",
        ] {
            assert_eq!(test_database_created_at(name), None, "{name}");
        }
    }

    #[tokio::test]
    async fn sweep_drops_only_aged_well_formed_test_databases() {
        let base_url = crate::test_database_url();
        let (maintenance_url, _) = fs3_store::maintenance_url(&base_url).unwrap();
        let admin = fs3_store::connect(&maintenance_url).await.unwrap();
        let now = SystemTime::now();
        let old_seconds = epoch_seconds(now) - ORPHAN_SWEEP_AGE.as_secs() - 1;
        let fresh_seconds = epoch_seconds(now);
        let old = format!("{TEST_DATABASE_PREFIX}{old_seconds}_{:032x}", unique_seed());
        let fresh = format!(
            "{TEST_DATABASE_PREFIX}{fresh_seconds}_{:032x}",
            unique_seed()
        );
        let unrelated = format!("fs3_sandbox_{old_seconds}_{:032x}", unique_seed());
        for name in [&old, &fresh, &unrelated] {
            fs3_store::create_database(&admin, name).await.unwrap();
        }

        let report = FreshDatabase::sweep_orphans_at(&base_url, now, ORPHAN_SWEEP_AGE)
            .await
            .unwrap();

        assert_eq!(report.threshold, ORPHAN_SWEEP_AGE);
        assert_eq!(report.swept, vec![old.clone()]);
        assert!(!fs3_store::database_exists(&admin, &old).await.unwrap());
        assert!(fs3_store::database_exists(&admin, &fresh).await.unwrap());
        assert!(
            fs3_store::database_exists(&admin, &unrelated)
                .await
                .unwrap()
        );
        fs3_store::drop_database(&admin, &fresh).await.unwrap();
        fs3_store::drop_database(&admin, &unrelated).await.unwrap();
        admin.close().await;
    }
}
