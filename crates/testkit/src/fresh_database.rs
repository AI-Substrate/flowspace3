//! Disposable, migrated Postgres databases for tests and hand-run sandboxes.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use fs3_store::{PgPool, StoreError};

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
        let name = database_name(label);
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

fn database_name(label: &str) -> String {
    let label: String = label
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .take(12)
        .collect();
    format!("fs3_{label}_{:032x}", unique_seed())
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
    use super::database_name;

    #[test]
    fn names_are_plain_bounded_and_unique() {
        let first = database_name("sandbox-with punctuation");
        let second = database_name("sandbox-with punctuation");
        assert_ne!(first, second);
        assert!(first.len() <= 63, "{first}");
        assert!(
            first
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_'),
            "{first}"
        );
    }
}
