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

use fs3_core::{BlobRef, Element, ElementKind, ElementTree, Span};
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

/// Insert or replace one element of `tree`, keyed by its content address.
///
/// The element is passed separately from the tree because a tree is nested and
/// this table is flat: a caller writes `tree.iter()`, one row per element. The
/// tree supplies the facts that are true of the whole file — path, content key,
/// parse health — which is why they are not repeated on every node.
///
/// 0001's table is the exemplar, not the schema: it has no column for
/// `sibling_order` or for the parent link, so a round-trip returns a flat list
/// in source order rather than the tree. Storing the tree shape is workshop
/// material for plan 002.
///
/// # Errors
/// [`StoreError::Query`] when the statement fails.
pub async fn upsert_element(
    pool: &PgPool,
    tree: &ElementTree,
    element: &Element,
) -> Result<(), StoreError> {
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
    .bind(tree.blob.as_str())
    .bind(&tree.path)
    .bind(&element.address)
    .bind(&element.subkind)
    .bind(element.kind.as_str())
    .bind(element.span.start_line as i32)
    .bind(element.span.end_line as i32)
    .bind(&element.raw_text)
    .bind(tree.has_error)
    .execute(pool)
    .await?;
    Ok(())
}

/// Every element recorded for one blob, flat, in source order.
///
/// # Errors
/// [`StoreError::Query`] on failure; [`StoreError::Corrupt`] when a stored row
/// cannot be read back as a domain element.
pub async fn elements_for_blob(pool: &PgPool, blob: &BlobRef) -> Result<Vec<Element>, StoreError> {
    let rows = sqlx::query(
        "SELECT qualified_name, ts_kind, kind, start_line, end_line, body
           FROM elements
          WHERE blob = $1
          ORDER BY start_line, qualified_name",
    )
    .bind(blob.as_str())
    .fetch_all(pool)
    .await?;

    rows.iter()
        .enumerate()
        .map(|(index, row)| element_from_row(row, index as u32))
        .collect()
}

/// Rebuild an element from its row.
///
/// `raw_hash` is not a stored column and does not need to be: it is derived
/// from the body, so reading it back and re-deriving it give the same digest by
/// construction. `sibling_order` IS lost — the flat table cannot express it —
/// so it is re-derived as position in source order.
fn element_from_row(row: &PgRow, source_order: u32) -> Result<Element, StoreError> {
    let kind: String = row.try_get("kind")?;
    Ok(Element::new(
        kind_from_str(&kind)?,
        row.try_get::<String, _>("ts_kind")?,
        // The declaration's own name is the last address segment.
        last_segment(row.try_get::<&str, _>("qualified_name")?),
        row.try_get::<String, _>("qualified_name")?,
        Span::new(
            row.try_get::<i32, _>("start_line")? as u32,
            row.try_get::<i32, _>("end_line")? as u32,
        ),
        row.try_get::<String, _>("body")?,
    )
    .with_sibling_order(source_order))
}

/// The declaration's own name within `src/foo.rs::Indexer::scan`.
fn last_segment(address: &str) -> &str {
    address
        .rsplit(fs3_core::ADDRESS_SEGMENT)
        .next()
        .unwrap_or(address)
}

fn kind_from_str(value: &str) -> Result<ElementKind, StoreError> {
    match value {
        "file" => Ok(ElementKind::File),
        "container" => Ok(ElementKind::Container),
        "function" => Ok(ElementKind::Function),
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
            ElementKind::File,
            ElementKind::Container,
            ElementKind::Function,
            ElementKind::Section,
        ] {
            assert_eq!(kind_from_str(kind.as_str()).unwrap(), kind);
        }
        // The spellings 0001 used, which migration 0002 renamed. A row that
        // still says `callable` means the migration did not run.
        assert!(kind_from_str("callable").is_err());
        assert!(kind_from_str("type").is_err());
        assert!(kind_from_str("block").is_err());
    }

    #[test]
    fn a_name_is_the_last_address_segment() {
        assert_eq!(last_segment("src/foo.rs::Indexer::scan"), "scan");
        // A file element's address has no segments at all.
        assert_eq!(last_segment("src/foo.rs"), "src/foo.rs");
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
