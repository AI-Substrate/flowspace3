//! Table health: dead rows and statistics staleness on the big tables, and
//! the ANALYZE that refreshes a table once bulk work has changed it.
//!
//! Default autovacuum thresholds never fire on million-row tables, and stale
//! statistics are how a one-row probe becomes a sequential scan (2026-10-14).
//! Migration 0031 lowers the thresholds; this is what reports on them and what
//! catches up after a bulk job without waiting for autovacuum's next visit.

use sqlx::Row;

use crate::{PgPool, StoreError};

/// The tables search and indexing live on, the ones worth watching.
pub const WATCHED_TABLES: [&str; 7] = [
    "elements",
    "embeddings_1024",
    "embeddings_1024_bq",
    "turns",
    "smart_content",
    "worktree_files",
    "jobs",
];

/// One table's maintenance state, as Postgres's statistics collector sees it.
#[derive(Clone, Debug, PartialEq)]
pub struct TableHealth {
    pub table: String,
    pub live_rows: i64,
    pub dead_rows: i64,
    /// Rows changed since the last ANALYZE, manual or automatic.
    pub changed_since_analyze: i64,
    /// Seconds since the last ANALYZE, manual or automatic; `None` if never.
    pub analyzed_secs_ago: Option<i64>,
    /// Seconds since the last VACUUM, manual or automatic; `None` if never.
    pub vacuumed_secs_ago: Option<i64>,
}

impl TableHealth {
    /// Dead rows as a share of live ones.
    #[must_use]
    pub fn dead_ratio(&self) -> f64 {
        self.dead_rows as f64 / self.live_rows.max(1) as f64
    }

    /// Rows changed since the last ANALYZE, as a share of live ones.
    #[must_use]
    pub fn stale_ratio(&self) -> f64 {
        self.changed_since_analyze as f64 / self.live_rows.max(1) as f64
    }
}

/// Health of every [`WATCHED_TABLES`] table, largest first.
///
/// # Errors
/// [`StoreError::Query`] when the read fails.
pub async fn table_health(pool: &PgPool) -> Result<Vec<TableHealth>, StoreError> {
    let rows = sqlx::query(
        "SELECT relname::text AS table_name, n_live_tup, n_dead_tup, n_mod_since_analyze,
                extract(epoch FROM now() - greatest(last_analyze, last_autoanalyze))::bigint
                    AS analyzed_secs_ago,
                extract(epoch FROM now() - greatest(last_vacuum, last_autovacuum))::bigint
                    AS vacuumed_secs_ago
           FROM pg_stat_user_tables
          WHERE schemaname = current_schema() AND relname = ANY($1)
          ORDER BY n_live_tup DESC",
    )
    .bind(&WATCHED_TABLES[..])
    .fetch_all(pool)
    .await?;
    rows.iter()
        .map(|row| {
            Ok(TableHealth {
                table: row.try_get("table_name")?,
                live_rows: row.try_get("n_live_tup")?,
                dead_rows: row.try_get("n_dead_tup")?,
                changed_since_analyze: row.try_get("n_mod_since_analyze")?,
                analyzed_secs_ago: row.try_get("analyzed_secs_ago")?,
                vacuumed_secs_ago: row.try_get("vacuumed_secs_ago")?,
            })
        })
        .collect()
}

/// Refresh one watched table's planner statistics.
///
/// # Errors
/// [`StoreError::Query`] when ANALYZE fails, or for a table that is not one
/// of [`WATCHED_TABLES`] (the name is spliced into the statement, so only
/// those names are accepted).
pub async fn analyze_table(pool: &PgPool, table: &str) -> Result<(), StoreError> {
    if !WATCHED_TABLES.contains(&table) {
        return Err(StoreError::Query(sqlx::Error::Protocol(format!(
            "{table:?} is not a watched table"
        ))));
    }
    sqlx::query(&format!("ANALYZE {table}"))
        .execute(pool)
        .await?;
    Ok(())
}
