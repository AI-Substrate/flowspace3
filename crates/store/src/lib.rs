//! The central Postgres + pgvector store (PRD req 4).
//!
//! There is no repository trait here, and no in-memory implementation. Postgres
//! is a requirement, not a variable — workshop 001 rule 3 refuses the
//! abstraction, and the refused-anti-patterns list names "repository-trait over
//! sqlx" specifically. Tests run against a real dockerized instance.
//!
//! sqlx never leaves this crate: the architecture check enforces it. What
//! callers get instead is the typed API in [`elements`], [`smart`],
//! [`embeddings`] and [`jobs`] — one function per *flow* the daemon actually
//! performs, not table-shaped CRUD. Every one of them takes and returns
//! `fs3_core` domain types, because a DTO layer between crates is another
//! refused anti-pattern.
//!
//! The schema those functions speak to is workshop 002
//! (`docs/plans/prd/workshops/002-pg-schema.md`); the guided tour of it is
//! `docs/services/store-schema.md`.
//!
//! Queries are runtime (`sqlx::query`) rather than the compile-time-checked
//! macros: the macros need a live database or a checked-in `.sqlx` cache at
//! *build* time, which would make `cargo build` depend on docker.

use sqlx::postgres::PgPoolOptions;

pub mod admin;
pub mod conversations;
pub mod elements;
pub mod embeddings;
pub mod ingest_cursors;
pub mod jobs;
pub mod messages;
pub mod read;
pub mod refs;
pub mod roots;
pub mod smart;
pub mod updates;

pub use admin::{
    SchemaStatus, create_database, database_exists, database_url, drop_database,
    is_missing_database, maintenance_url, schema_current,
};
pub use conversations::{
    AnchorFilter, Appended, ConversationSummary, Removed, TurnOutline, append_turns,
    delete_conversation, list_conversations, outline, upsert_conversation, window,
};
pub use elements::{blobs_with_parser_version, get_elements, upsert_element_tree};
pub use embeddings::{
    EMBEDDING_DIMENSIONS, NewEmbedding, SearchFilters, SearchHit, SimilarElement, SourceKind,
    anchor_has_vectors, embedding_models, existing_embedding_hashes, put_embeddings,
    query_embeddings, search_elements,
};
pub use jobs::{
    JOB_PRIORITY_DEFAULT, JOB_PRIORITY_NEW_WORKTREE_SCAN, Job, JobPriority, QueueDepth, claim_job,
    claim_jobs, complete_job, enqueue_job, enqueue_job_with_priority, fail_job, jobs_remaining,
    last_failure, park_job, queue_depth, requeue_failed, requeue_running, retry_job,
};
pub use messages::{ack_message, live_messages, sync_messages};
pub use read::{
    IndexedFile, count_files_under, files_at_path, files_under, latest_summary,
    parser_versions_for_blob, repo_identities, worktree_containing,
};
pub use refs::{
    RegisteredWorktree, WorktreePath, find_worktree, list_worktrees, register_worktree,
    sync_worktree_files, worktree_file_map, worktree_paths_for_blob,
};
pub use roots::{
    Reclaimed, Removal, collect_garbage, raw_hash_is_referenced, reclaimable,
    referenced_source_hashes, remove_root, worktree_exists,
};
pub use smart::{
    MissingEnrichment, MissingVector, get_smart_content, missing_embeddings, missing_enrichment,
    put_smart_content,
};
pub use updates::{
    claim_check, claim_check_now, record_blocked, record_clear, record_on_disk, record_seen,
    record_swapped, update_state,
};

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
    ///
    /// Carries the underlying message as text rather than as a `#[source]`.
    /// thiserror's `#[from]` makes a field a source too, and a variant whose
    /// own `Display` also interpolates it then prints TWICE under any
    /// chain-walking formatter — which `flowspace3`'s `{error:#}` is. Jordan
    /// saw exactly that on 2026-08-27: "…migration 8 was previously applied but
    /// is missing in the resolved migrations: migration 8 was previously
    /// applied but is missing in the resolved migrations".
    #[error("migrations failed: {0}")]
    Migrate(String),
    /// A row in the database does not match the domain model.
    #[error("row is not a valid element: {0}")]
    Corrupt(fs3_core::Error),
    /// A name or URL the store was asked to act on cannot be used as given.
    ///
    /// Its own variant because it is the caller's to fix and it never reaches
    /// Postgres: `CREATE DATABASE` takes no bind parameters, so an identifier
    /// that would need escaping is refused *before* a statement is built
    /// rather than after the server rejects it.
    #[error("{0}")]
    InvalidName(String),
    /// A vector was offered to a table of a different width.
    ///
    /// Its own variant rather than a database error because the caller can act
    /// on it: the fix is a different embedding model or a new
    /// `embeddings_<dim>` table (decision D3), not a retry.
    #[error(
        "embedding has {actual} dimensions but embeddings_{expected} holds {expected}-wide \
         vectors — a model of another width needs its own table (workshop 002, decision D3)"
    )]
    Dimensions {
        /// The width the target table holds.
        expected: usize,
        /// The width the caller offered.
        actual: usize,
    },
    /// A session was offered under a different conversation than the one it is
    /// already tailing.
    ///
    /// Its own variant rather than a silent update because the caller can act
    /// on it and a retry cannot: `ingest_ledger` is keyed by session and
    /// carries no conversation, so its rows would not move with the rebind —
    /// the ledger would insist every record is stored while the newly named
    /// conversation held nothing, forever, with every call reporting success.
    /// A session moving conversations is a bug in resolution, not an update.
    #[error(
        "session {harness}/{session_id} already tails conversation {stored}, not {offered} — \
         a session cannot move conversations: its ledger is keyed by session and would not \
         move with it, leaving {offered} permanently empty while every call reported success"
    )]
    SessionRebound {
        /// The store whose session this is.
        harness: String,
        /// The session that is already tailing something else.
        session_id: String,
        /// The conversation it is already tailing.
        stored: String,
        /// The conversation it was offered under.
        offered: String,
    },
}

/// Hand-written so `?` still works on a `MigrateError` while
/// [`StoreError::Migrate`] keeps its detail without also carrying a source.
impl From<sqlx::migrate::MigrateError> for StoreError {
    fn from(error: sqlx::migrate::MigrateError) -> Self {
        StoreError::Migrate(error.to_string())
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unreachable_names_the_command_that_fixes_it() {
        let error = StoreError::Unreachable {
            url: "postgres://x".into(),
            source: sqlx::Error::PoolClosed,
        };
        assert!(error.to_string().contains("docker compose up -d"));
    }

    #[test]
    fn a_width_mismatch_names_the_decision_that_explains_it() {
        let error = StoreError::Dimensions {
            expected: EMBEDDING_DIMENSIONS,
            actual: 32,
        };
        let message = error.to_string();
        assert!(message.contains("32 dimensions"), "{message}");
        assert!(message.contains("embeddings_1024"), "{message}");
    }
}
