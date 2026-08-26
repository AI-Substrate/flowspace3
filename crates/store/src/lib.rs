//! The central Postgres + pgvector store (PRD req 4).
//!
//! There is no repository trait here, and no in-memory implementation. Postgres
//! is a requirement, not a variable — workshop 001 rule 3 refuses the
//! abstraction, and the refused-anti-patterns list names "repository-trait over
//! sqlx" specifically. Tests run against a real dockerized instance.
//!
//! Queries are runtime (`sqlx::query`) rather than the compile-time-checked
//! macros: the macros need a live database or a checked-in `.sqlx` cache at
//! *build* time, which would make `cargo build` depend on docker.

use fs3_core::{BlobRef, Element, ElementKind};
use sqlx::Row;
use sqlx::postgres::{PgPoolOptions, PgRow};

// The store owns the sqlx edge, so every other crate speaks to Postgres through
// this re-export rather than depending on sqlx itself.
pub use sqlx::PgPool;

/// Migrations, embedded at compile time. No migration files at runtime.
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// The exact command that brings the store up. Named in every connection
/// failure so a missing stack is never a puzzle.
pub const COMPOSE_UP: &str = "docker compose up -d";

/// How long [`connect`] waits before declaring the store unreachable.
///
/// Short on purpose: the common cause is a stopped compose stack, and thirty
/// seconds of silence per test is a worse answer than five seconds and the
/// command that fixes it.
pub const CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// Something went wrong talking to the store.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// Could not reach Postgres. Carries the exact command that fixes it.
    #[error(
        "cannot reach Postgres at {url}: {source}\n\
         The compose stack is probably not running. Start it with:\n    {COMPOSE_UP}"
    )]
    Unreachable {
        /// The connection URL that was tried.
        url: String,
        /// The underlying sqlx failure.
        source: sqlx::Error,
    },
    /// A query or migration failed.
    #[error("store query failed: {0}")]
    Query(#[from] sqlx::Error),
    /// Migrations could not be applied.
    #[error("migrations failed: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),
    /// A row in the database does not match the domain model.
    #[error("row is not a valid element: {0}")]
    Corrupt(fs3_core::Error),
}

/// Connect eagerly, proving the store is reachable before anything else starts.
///
/// # Errors
/// [`StoreError::Unreachable`] naming [`COMPOSE_UP`] when Postgres is not there.
pub async fn connect(url: &str) -> Result<PgPool, StoreError> {
    PgPoolOptions::new()
        .max_connections(8)
        .acquire_timeout(CONNECT_TIMEOUT)
        .connect(url)
        .await
        .map_err(|source| StoreError::Unreachable {
            url: url.to_string(),
            source,
        })
}

/// Build a pool without touching the network.
///
/// The daemon uses this so that wiring, and answering `GET /health`, do not
/// require the database to be reachable — connections are established on first
/// use.
///
/// The acquire timeout matches [`connect`] deliberately: the two constructors
/// must not disagree about how long "unreachable" takes. Without it the first
/// use of an absent store waits sqlx's thirty-second default before saying so,
/// which is the silence [`CONNECT_TIMEOUT`] exists to refuse — and the daemon's
/// boot migration is exactly such a first use.
pub fn connect_lazy(url: &str) -> Result<PgPool, StoreError> {
    PgPoolOptions::new()
        .max_connections(8)
        .acquire_timeout(CONNECT_TIMEOUT)
        .connect_lazy(url)
        .map_err(|source| StoreError::Unreachable {
            url: url.to_string(),
            source,
        })
}

/// Apply all pending migrations.
///
/// # Errors
/// [`StoreError::Migrate`] when a migration fails or the applied set diverges.
pub async fn migrate(pool: &PgPool) -> Result<(), StoreError> {
    MIGRATOR.run(pool).await?;
    Ok(())
}

/// Insert or replace one element, keyed by its content address.
///
/// # Errors
/// [`StoreError::Query`] when the statement fails.
pub async fn upsert_element(pool: &PgPool, element: &Element) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO elements
           (blob, path, qualified_name, ts_kind, kind, start_line, end_line, body, has_error)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
         ON CONFLICT (blob, qualified_name, start_line) DO UPDATE SET
           path = EXCLUDED.path,
           ts_kind = EXCLUDED.ts_kind,
           kind = EXCLUDED.kind,
           end_line = EXCLUDED.end_line,
           body = EXCLUDED.body,
           has_error = EXCLUDED.has_error",
    )
    .bind(element.blob.as_str())
    .bind(&element.path)
    .bind(&element.qualified_name)
    .bind(&element.ts_kind)
    .bind(element.kind.as_str())
    .bind(element.start_line as i32)
    .bind(element.end_line as i32)
    .bind(&element.text)
    .bind(element.has_error)
    .execute(pool)
    .await?;
    Ok(())
}

/// Every element recorded for one blob, in source order.
///
/// # Errors
/// [`StoreError::Query`] on failure; [`StoreError::Corrupt`] when a stored row
/// cannot be read back as a domain element.
pub async fn elements_for_blob(pool: &PgPool, blob: &BlobRef) -> Result<Vec<Element>, StoreError> {
    let rows = sqlx::query(
        "SELECT blob, path, qualified_name, ts_kind, kind, start_line, end_line, body, has_error
           FROM elements
          WHERE blob = $1
          ORDER BY start_line, qualified_name",
    )
    .bind(blob.as_str())
    .fetch_all(pool)
    .await?;

    rows.iter().map(element_from_row).collect()
}

fn element_from_row(row: &PgRow) -> Result<Element, StoreError> {
    let blob: String = row.try_get("blob")?;
    let kind: String = row.try_get("kind")?;
    Ok(Element {
        blob: BlobRef::new(blob).map_err(StoreError::Corrupt)?,
        path: row.try_get("path")?,
        qualified_name: row.try_get("qualified_name")?,
        ts_kind: row.try_get("ts_kind")?,
        kind: kind_from_str(&kind)?,
        start_line: row.try_get::<i32, _>("start_line")? as u32,
        end_line: row.try_get::<i32, _>("end_line")? as u32,
        text: row.try_get("body")?,
        has_error: row.try_get("has_error")?,
    })
}

fn kind_from_str(value: &str) -> Result<ElementKind, StoreError> {
    match value {
        "callable" => Ok(ElementKind::Callable),
        "type" => Ok(ElementKind::Type),
        "section" => Ok(ElementKind::Section),
        other => Err(StoreError::Corrupt(fs3_core::Error::InvalidConfig(
            format!("unknown element kind {other:?}"),
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_round_trips_through_its_stored_spelling() {
        for kind in [
            ElementKind::Callable,
            ElementKind::Type,
            ElementKind::Section,
        ] {
            assert_eq!(kind_from_str(kind.as_str()).unwrap(), kind);
        }
        assert!(kind_from_str("block").is_err());
    }

    #[test]
    fn unreachable_names_the_command_that_fixes_it() {
        let error = StoreError::Unreachable {
            url: "postgres://x".into(),
            source: sqlx::Error::PoolClosed,
        };
        assert!(error.to_string().contains("docker compose up -d"));
    }
}
