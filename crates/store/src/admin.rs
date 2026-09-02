//! Administration: does the database exist, is its schema current, make it so.
//!
//! Database CREATE/DROP is process-wide serialised here, at the lowest shared
//! API, so test helpers and one-shot CLI callers cannot bypass coordination.
//! `FS3_DB_MUTATION_CONCURRENCY` permits 1 or 2 concurrent mutations and
//! defaults to 1. Each process owns one semaphore; separate processes remain
//! independent.
//!
//! Everything here is *control plane* — the operations that logically precede a
//! daemon existing at all. That is why they are a separate module from the
//! flows: `elements`, `smart`, `embeddings` and `jobs` all assume a migrated
//! database, and these functions are how it gets to be one.
//!
//! Single responsibility, one function per step (Jordan's composition ruling,
//! 2026-08-26): each of these does exactly one thing and reports what it found.
//! `flowspace3 doctor` ORCHESTRATES them — walking engine → stack → database →
//! schema, repairing as it goes — and implements none of them. A second
//! implementation of "is the schema current" living inside doctor is exactly
//! the drift this split refuses.
//!
//! # Why the fast check exists
//!
//! Every db-touching command runs [`schema_current`] before it works, and
//! rejects a stale database with `FS3-E-STORE-SCHEMA-STALE` naming doctor. The
//! alternative — letting the command run and fail on a missing column — reports
//! the symptom (`column "enrich" does not exist`) instead of the cause, and the
//! reader has to know the migration history to translate. One cheap query buys
//! an error that says what to do.
//!
//! The check is one indexed read of `_sqlx_migrations`, not a schema
//! comparison: the embedded [`crate::MIGRATOR`] is the source of truth for what
//! *should* be applied, and the table records what *is*.

use std::sync::OnceLock;

use sqlx::Row;
use tokio::sync::{Semaphore, SemaphorePermit};

use crate::{PgPool, StoreError};

const DEFAULT_DATABASE_MUTATION_CONCURRENCY: usize = 1;
const MAX_DATABASE_MUTATION_CONCURRENCY: usize = 2;
static DATABASE_MUTATION_LIMIT: OnceLock<Semaphore> = OnceLock::new();

#[cfg(test)]
mod create_test_hook {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    static IN_FLIGHT: AtomicUsize = AtomicUsize::new(0);
    static MAX_IN_FLIGHT: AtomicUsize = AtomicUsize::new(0);

    pub(super) async fn enter() -> InFlight {
        let active = IN_FLIGHT.fetch_add(1, Ordering::SeqCst) + 1;
        MAX_IN_FLIGHT.fetch_max(active, Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(25)).await;
        InFlight
    }

    pub(super) fn reset() {
        IN_FLIGHT.store(0, Ordering::SeqCst);
        MAX_IN_FLIGHT.store(0, Ordering::SeqCst);
    }

    pub(super) fn maximum() -> usize {
        MAX_IN_FLIGHT.load(Ordering::SeqCst)
    }

    pub(super) struct InFlight;

    impl Drop for InFlight {
        fn drop(&mut self) {
            IN_FLIGHT.fetch_sub(1, Ordering::SeqCst);
        }
    }
}

fn database_mutation_concurrency() -> usize {
    match std::env::var("FS3_DB_MUTATION_CONCURRENCY") {
        Ok(raw) => raw
            .parse()
            .ok()
            .filter(|value| (1..=MAX_DATABASE_MUTATION_CONCURRENCY).contains(value))
            .unwrap_or_else(|| panic!("FS3_DB_MUTATION_CONCURRENCY must be 1 or 2, got {raw:?}")),
        Err(std::env::VarError::NotPresent) => DEFAULT_DATABASE_MUTATION_CONCURRENCY,
        Err(error) => panic!("reading FS3_DB_MUTATION_CONCURRENCY: {error}"),
    }
}

async fn database_mutation_permit() -> SemaphorePermit<'static> {
    DATABASE_MUTATION_LIMIT
        .get_or_init(|| Semaphore::new(database_mutation_concurrency()))
        .acquire()
        .await
        .expect("the database mutation semaphore is never closed")
}

/// What [`schema_current`] found.
///
/// Carries the versions rather than just a boolean because the caller reports
/// them: "schema behind → doctor applied 0006-0007" is a useful line, and
/// "schema behind" alone is not.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchemaStatus {
    /// Migration versions this binary carries, ascending.
    pub embedded: Vec<i64>,
    /// Migration versions the database has applied, ascending.
    pub applied: Vec<i64>,
    /// Embedded versions the database has not applied, ascending.
    ///
    /// Empty means current. Non-empty is exactly what [`crate::migrate`] would
    /// apply.
    pub missing: Vec<i64>,
}

impl SchemaStatus {
    /// Whether the database has everything this binary carries.
    #[must_use]
    pub fn is_current(&self) -> bool {
        self.missing.is_empty()
    }

    /// Versions the DATABASE has that this binary does not.
    ///
    /// A newer daemon migrated this database and an older one is now looking at
    /// it. Not an error here — the old binary's queries may still work — but a
    /// fact worth reporting rather than hiding, because it explains a column
    /// nobody expected.
    #[must_use]
    pub fn ahead(&self) -> Vec<i64> {
        self.applied
            .iter()
            .filter(|version| !self.embedded.contains(version))
            .copied()
            .collect()
    }

    /// `0004-0005`-style summary of what is missing, for an error message.
    #[must_use]
    pub fn missing_summary(&self) -> String {
        match self.missing.as_slice() {
            [] => "nothing".to_string(),
            [one] => format!("{one:04}"),
            [first, .., last] => format!("{first:04}-{last:04}"),
        }
    }

    /// This status as the domain's skew type, for a binary reporting
    /// `binary_version`.
    ///
    /// The decision and the words live in [`fs3_core::skew`]; this is the
    /// adapter that fills them in, so boot, doctor and the message producer
    /// cannot phrase the same finding three different ways.
    #[must_use]
    pub fn skew(&self, binary_version: &str) -> fs3_core::SchemaSkew {
        fs3_core::SchemaSkew {
            binary_version: binary_version.to_string(),
            bundled_highest: self.embedded.last().copied(),
            extra: self.ahead(),
        }
    }
}

/// Compare the embedded migrations against what the database has applied.
///
/// A database with no `_sqlx_migrations` table at all is not an error: it is a
/// fresh database, and every embedded version is missing. Treating that as a
/// failure would make the first run of a new stack report a broken store rather
/// than an unmigrated one.
///
/// # Errors
/// [`StoreError::Unreachable`] when Postgres does not answer;
/// [`StoreError::Query`] when the read itself fails.
pub async fn schema_current(pool: &PgPool) -> Result<SchemaStatus, StoreError> {
    let mut embedded: Vec<i64> = crate::MIGRATOR
        .iter()
        .map(|migration| migration.version)
        .collect();
    embedded.sort_unstable();

    // `to_regclass` answers NULL rather than raising for an absent table, so a
    // fresh database costs one query here instead of a caught error.
    let exists: Option<String> = sqlx::query("SELECT to_regclass('_sqlx_migrations')::text")
        .fetch_one(pool)
        .await?
        .try_get(0)?;

    let mut applied: Vec<i64> = if exists.is_some() {
        sqlx::query("SELECT version FROM _sqlx_migrations WHERE success ORDER BY version")
            .fetch_all(pool)
            .await?
            .iter()
            .map(|row| row.try_get::<i64, _>("version"))
            .collect::<Result<_, _>>()?
    } else {
        Vec::new()
    };
    applied.sort_unstable();

    let missing = embedded
        .iter()
        .filter(|version| !applied.contains(version))
        .copied()
        .collect();

    Ok(SchemaStatus {
        embedded,
        applied,
        missing,
    })
}

/// Whether a database of this name exists on the server `admin` is connected to.
///
/// `admin` must be a pool onto a database that already exists — the
/// maintenance database. [`maintenance_url`] splits a normal fs3 URL into the
/// pair this needs.
///
/// # Errors
/// [`StoreError::Query`] when the read fails, including when the server does
/// not answer.
pub async fn database_exists(admin: &PgPool, name: &str) -> Result<bool, StoreError> {
    let row = sqlx::query("SELECT 1 FROM pg_database WHERE datname = $1")
        .bind(name)
        .fetch_optional(admin)
        .await?;
    Ok(row.is_some())
}

/// Database names beginning with `prefix`, ordered for deterministic reports.
///
/// `admin` must be connected to the maintenance database. Prefix matching is
/// literal rather than `LIKE`, so `_` in an fs3 namespace is not a wildcard.
///
/// # Errors
/// [`StoreError::Query`] when the catalog read fails.
pub async fn database_names_with_prefix(
    admin: &PgPool,
    prefix: &str,
) -> Result<Vec<String>, StoreError> {
    let rows = sqlx::query(
        "SELECT datname FROM pg_database WHERE left(datname, length($1)) = $1 ORDER BY datname",
    )
    .bind(prefix)
    .fetch_all(admin)
    .await?;
    rows.iter()
        .map(|row| {
            row.try_get::<String, _>("datname")
                .map_err(StoreError::from)
        })
        .collect()
}

/// Idle database names beginning with `prefix`, ordered for deterministic reports.
///
/// `admin` must be connected to the maintenance database. A database with any
/// active backend is excluded so an orphan sweep cannot select a live sandbox.
///
/// # Errors
/// [`StoreError::Query`] when the catalog read fails.
pub async fn idle_database_names_with_prefix(
    admin: &PgPool,
    prefix: &str,
) -> Result<Vec<String>, StoreError> {
    let rows = sqlx::query(
        "SELECT datname FROM pg_stat_database \
         WHERE left(datname, length($1)) = $1 AND numbackends = 0 ORDER BY datname",
    )
    .bind(prefix)
    .fetch_all(admin)
    .await?;
    rows.iter()
        .map(|row| {
            row.try_get::<String, _>("datname")
                .map_err(StoreError::from)
        })
        .collect()
}

/// SQL template used by [`drop_database_if_idle`].
///
/// Exposed only so the cross-crate sweep regression guard can assert that the
/// fail-safe statement remains unforced.
#[doc(hidden)]
pub const DROP_DATABASE_IF_IDLE_SQL: &str = "DROP DATABASE IF EXISTS \"{database_name}\"";

/// Drop a database only while it has no active backends.
///
/// The catalog predicate excludes live databases. The unforced `DROP` closes
/// the race between that read and the statement: if a backend connects in the
/// meantime, Postgres refuses the drop and this returns `Ok(false)`.
///
/// # Errors
/// [`StoreError::InvalidName`] when the name is not a plain identifier;
/// [`StoreError::Query`] when the catalog read or drop fails for another reason.
pub async fn drop_database_if_idle(admin: &PgPool, name: &str) -> Result<bool, StoreError> {
    validate_database_name(name)?;
    let _permit = database_mutation_permit().await;
    let backends = sqlx::query_scalar::<_, i64>(
        "SELECT COALESCE(\
             (SELECT numbackends::bigint FROM pg_stat_database WHERE datname = $1), 0)",
    )
    .bind(name)
    .fetch_one(admin)
    .await?;
    if backends != 0 {
        return Ok(false);
    }

    match sqlx::query(&DROP_DATABASE_IF_IDLE_SQL.replace("{database_name}", name))
        .execute(admin)
        .await
    {
        Ok(_) => Ok(true),
        Err(sqlx::Error::Database(error)) if error.code().as_deref() == Some("55006") => Ok(false),
        Err(error) => Err(StoreError::from(error)),
    }
}

/// Create an empty database.
///
/// `CREATE DATABASE` takes no bind parameters, so the name is interpolated —
/// which is why it is validated first. The rule is deliberately narrower than
/// Postgres': fs3 database names come from a config URL, and anything outside
/// `[A-Za-z0-9_]` is far more likely to be a mis-parsed URL than a deliberate
/// exotic identifier.
///
/// # Errors
/// [`StoreError::InvalidName`] when the name is not a plain identifier;
/// [`StoreError::Query`] when the statement fails, including when another
/// process created the database first.
pub async fn create_database(admin: &PgPool, name: &str) -> Result<(), StoreError> {
    validate_database_name(name)?;
    let _permit = database_mutation_permit().await;
    #[cfg(test)]
    let _test_hook = create_test_hook::enter().await;
    sqlx::query(&format!("CREATE DATABASE \"{name}\""))
        .execute(admin)
        .await?;
    Ok(())
}

/// Drop a database and terminate any sessions still connected to it.
///
/// The force is intentional for disposable databases: a crashed worker may
/// leave a pool alive, and cleanup must not silently preserve the database.
///
/// # Errors
/// [`StoreError::InvalidName`] when the name is not a plain identifier;
/// [`StoreError::Query`] when Postgres refuses the operation.
pub async fn drop_database(admin: &PgPool, name: &str) -> Result<(), StoreError> {
    validate_database_name(name)?;
    let _permit = database_mutation_permit().await;
    sqlx::query(&format!("DROP DATABASE IF EXISTS \"{name}\" WITH (FORCE)"))
        .execute(admin)
        .await?;
    Ok(())
}

/// Replace the database path in a Postgres URL, preserving its query string.
///
/// # Errors
/// [`StoreError::InvalidName`] when either the URL or replacement name is not
/// usable as a database identifier.
pub fn database_url(url: &str, name: &str) -> Result<String, StoreError> {
    validate_database_name(name)?;
    let (maintenance, _) = maintenance_url(url)?;
    let (prefix, query) = maintenance
        .split_once('?')
        .map_or((maintenance.as_str(), None), |(prefix, query)| {
            (prefix, Some(query))
        });
    let authority_end = prefix
        .rfind('/')
        .ok_or_else(|| StoreError::InvalidName(format!("{url:?} names no database")))?;
    Ok(match query {
        Some(query) => format!("{}/{name}?{query}", &prefix[..authority_end]),
        None => format!("{}/{name}", &prefix[..authority_end]),
    })
}

/// Split a database URL into `(maintenance url, database name)`.
///
/// The maintenance URL points at `postgres`, the database every server has, so
/// a caller can connect *somewhere* in order to ask about — or create — the one
/// that is missing. Pure string surgery: no connection is made here, so doctor
/// can build both halves before deciding to use either.
///
/// # Errors
/// [`StoreError::InvalidName`] when the URL carries no database path segment.
pub fn maintenance_url(url: &str) -> Result<(String, String), StoreError> {
    /// Every Postgres server has this database; connecting to it is how you ask
    /// about the others.
    const MAINTENANCE_DATABASE: &str = "postgres";

    let (scheme, rest) = url
        .split_once("://")
        .ok_or_else(|| StoreError::InvalidName(format!("{url:?} is not a postgres:// URL")))?;

    // The database is the first path segment after the authority; a query
    // string (`?sslmode=…`) rides along and must be preserved.
    let (authority, tail) = match rest.split_once('/') {
        Some(pair) => pair,
        None => {
            return Err(StoreError::InvalidName(format!(
                "{url:?} names no database — expected postgres://host:port/database"
            )));
        }
    };
    let (name, query) = match tail.split_once('?') {
        Some((name, query)) => (name, Some(query)),
        None => (tail, None),
    };
    if name.is_empty() {
        return Err(StoreError::InvalidName(format!(
            "{url:?} names no database — expected postgres://host:port/database"
        )));
    }

    let maintenance = match query {
        Some(query) => format!("{scheme}://{authority}/{MAINTENANCE_DATABASE}?{query}"),
        None => format!("{scheme}://{authority}/{MAINTENANCE_DATABASE}"),
    };
    Ok((maintenance, name.to_string()))
}

/// Refuse a name that would have to be escaped to be safe.
fn validate_database_name(name: &str) -> Result<(), StoreError> {
    let plain = !name.is_empty()
        && name.len() <= 63
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    if plain {
        Ok(())
    } else {
        Err(StoreError::InvalidName(format!(
            "{name:?} is not a plain database name; fs3 accepts letters, digits, underscore and \
             hyphen, up to 63 characters"
        )))
    }
}

/// Whether an error is Postgres' "that database does not exist" (SQLSTATE 3D000).
///
/// Doctor asks this of a *failed connection attempt*: the database being absent
/// is the one connection failure that has an automatic repair, and every other
/// one is a genuine "the server is not there", whose fix is to start the stack.
/// Without the distinction, doctor would either try to create a database on a
/// server that is down, or report a fixable absence as an outage.
#[must_use]
pub fn is_missing_database(error: &StoreError) -> bool {
    /// `invalid_catalog_name` — Postgres' code for connecting to a database
    /// that is not there.
    const INVALID_CATALOG_NAME: &str = "3D000";

    let inner = match error {
        StoreError::Query(inner) => inner,
        StoreError::Unreachable { source, .. } => source,
        _ => return false,
    };
    inner
        .as_database_error()
        .and_then(|db| db.code())
        .as_deref()
        == Some(INVALID_CATALOG_NAME)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(embedded: &[i64], applied: &[i64]) -> SchemaStatus {
        SchemaStatus {
            embedded: embedded.to_vec(),
            applied: applied.to_vec(),
            missing: embedded
                .iter()
                .filter(|version| !applied.contains(version))
                .copied()
                .collect(),
        }
    }

    /// The mapping the whole binary-older-than-database story rests on
    /// (req-0061): the store measures, `fs3_core::SchemaSkew` decides and
    /// speaks.
    #[test]
    fn a_database_ahead_of_the_binary_becomes_a_skew_carrying_both_sides() {
        let skew = status(&[1, 2, 3], &[1, 2, 3, 8, 9]).skew("0.1.0");

        assert!(skew.is_skewed());
        assert_eq!(skew.extra, vec![8, 9]);
        assert_eq!(skew.bundled_highest, Some(3));
        assert_eq!(skew.binary_version, "0.1.0");
    }

    /// The trap this ordering exists to close: a database that is AHEAD has
    /// nothing missing, so `is_current` is true and every caller that asked
    /// only that question reported a healthy schema.
    #[test]
    fn an_ahead_database_looks_current_which_is_why_skew_is_asked_first() {
        let ahead = status(&[1, 2], &[1, 2, 8]);

        assert!(ahead.is_current(), "this is the misleading part, not a bug");
        assert!(
            ahead.skew("0.1.0").is_skewed(),
            "and this is what catches it"
        );
    }

    #[test]
    fn a_database_merely_behind_is_not_skewed() {
        let behind = status(&[1, 2, 3], &[1]);

        assert!(!behind.is_current());
        assert!(
            !behind.skew("0.3.0").is_skewed(),
            "behind is the ordinary case — migrating fixes it"
        );
    }

    #[test]
    fn maintenance_url_keeps_the_authority_and_swaps_the_database() {
        let (admin, name) =
            maintenance_url("postgres://flowspace3:pw@127.0.0.1:5433/flowspace3").unwrap();
        assert_eq!(admin, "postgres://flowspace3:pw@127.0.0.1:5433/postgres");
        assert_eq!(name, "flowspace3");
    }

    /// A query string is connection-critical (`sslmode=require`), so dropping it
    /// on the maintenance leg would make doctor fail where the daemon succeeds.
    #[test]
    fn maintenance_url_preserves_the_query_string() {
        let (admin, name) =
            maintenance_url("postgres://h/fs3?sslmode=require&connect_timeout=5").unwrap();
        assert_eq!(
            admin,
            "postgres://h/postgres?sslmode=require&connect_timeout=5"
        );
        assert_eq!(name, "fs3");
    }

    #[test]
    fn database_url_replaces_name_and_preserves_connection_query() {
        assert_eq!(
            database_url(
                "postgres://flowspace3:pw@127.0.0.1:5433/original?sslmode=require",
                "fs3_sandbox_123"
            )
            .unwrap(),
            "postgres://flowspace3:pw@127.0.0.1:5433/fs3_sandbox_123?sslmode=require"
        );
    }

    #[test]
    fn maintenance_url_refuses_a_url_that_names_no_database() {
        for url in [
            "postgres://127.0.0.1:5433",
            "postgres://127.0.0.1:5433/",
            "nonsense",
        ] {
            assert!(
                maintenance_url(url).is_err(),
                "{url:?} names no database and must be refused"
            );
        }
    }

    /// `CREATE DATABASE` cannot take a bind parameter, so the name is
    /// interpolated — which makes this validation the only thing between a
    /// config URL and a statement.
    #[test]
    fn database_names_that_would_need_escaping_are_refused() {
        assert!(validate_database_name("flowspace3").is_ok());
        assert!(validate_database_name("fs3_migrations_0001").is_ok());
        assert!(validate_database_name("fs3-test").is_ok());
        for bad in [
            "",
            "has space",
            "quote\"inside",
            "semi;colon",
            "back`tick",
            "\u{e9}accent",
        ] {
            assert!(
                validate_database_name(bad).is_err(),
                "{bad:?} must be refused"
            );
        }
        assert!(validate_database_name(&"x".repeat(64)).is_err());
    }

    #[test]
    fn missing_summary_reads_like_a_migration_range() {
        let status = |missing: Vec<i64>| SchemaStatus {
            embedded: vec![1, 2, 3, 4, 5],
            applied: vec![],
            missing,
        };
        assert_eq!(status(vec![]).missing_summary(), "nothing");
        assert_eq!(status(vec![6]).missing_summary(), "0006");
        assert_eq!(status(vec![4, 5]).missing_summary(), "0004-0005");
    }

    #[tokio::test]
    async fn eight_create_paths_are_serialised() {
        use std::time::{SystemTime, UNIX_EPOCH};

        const WORKERS: usize = 8;
        create_test_hook::reset();
        let base_url = std::env::var("FS3_TEST_DATABASE_URL")
            .expect("FS3_TEST_DATABASE_URL must name the disposable test server");
        let (maintenance_url, _) = maintenance_url(&base_url).unwrap();
        let admin = crate::connect(&maintenance_url).await.unwrap();
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let names: Vec<_> = (0..WORKERS)
            .map(|worker| format!("fs3_storelock_{seed:032x}_{worker}"))
            .collect();
        let mut tasks = Vec::with_capacity(WORKERS);
        for name in &names {
            let admin = admin.clone();
            let name = name.clone();
            tasks.push(tokio::spawn(
                async move { create_database(&admin, &name).await },
            ));
        }
        for task in tasks {
            task.await.unwrap().unwrap();
        }
        for name in &names {
            drop_database(&admin, name).await.unwrap();
        }
        admin.close().await;

        let maximum = create_test_hook::maximum();
        assert!(
            maximum <= database_mutation_concurrency(),
            "observed {maximum} concurrent CREATE DATABASE operations"
        );
    }

    #[test]
    fn database_mutations_are_serialised_across_tokio_runtimes() {
        use std::sync::{Arc, Barrier};
        use std::thread;
        use std::time::{SystemTime, UNIX_EPOCH};

        const WORKERS: usize = 8;
        create_test_hook::reset();
        let base_url = std::env::var("FS3_TEST_DATABASE_URL")
            .expect("FS3_TEST_DATABASE_URL must name the disposable test server");
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let names: Vec<_> = (0..WORKERS)
            .map(|worker| format!("fs3_storelock_{seed:032x}_{worker}"))
            .collect();
        let barrier = Arc::new(Barrier::new(WORKERS));
        let threads: Vec<_> = names
            .iter()
            .cloned()
            .map(|name| {
                let barrier = Arc::clone(&barrier);
                let base_url = base_url.clone();
                thread::spawn(move || {
                    tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .unwrap()
                        .block_on(async move {
                            let (maintenance_url, _) = maintenance_url(&base_url).unwrap();
                            let admin = crate::connect(&maintenance_url).await.unwrap();
                            barrier.wait();
                            let result = create_database(&admin, &name).await;
                            admin.close().await;
                            result
                        })
                })
            })
            .collect();
        let results: Vec<_> = threads.into_iter().map(thread::JoinHandle::join).collect();

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let (maintenance_url, _) = maintenance_url(&base_url).unwrap();
            let admin = crate::connect(&maintenance_url).await.unwrap();
            for name in &names {
                drop_database(&admin, name).await.unwrap();
            }
            admin.close().await;
        });
        for result in results {
            result.expect("create worker panicked").unwrap();
        }

        let maximum = create_test_hook::maximum();
        assert!(
            maximum <= database_mutation_concurrency(),
            "observed {maximum} concurrent CREATE DATABASE operations across Tokio runtimes"
        );
    }

    #[test]
    fn a_database_ahead_of_the_binary_is_reported_not_hidden() {
        let status = SchemaStatus {
            embedded: vec![1, 2],
            applied: vec![1, 2, 3],
            missing: vec![],
        };
        assert!(status.is_current());
        assert_eq!(status.ahead(), vec![3]);
    }
}
