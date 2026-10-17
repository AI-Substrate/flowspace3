//! The scope cache: each search scope's admitted sources, kept between
//! searches.
//!
//! A scoped vector search starts by collecting every source its scope admits
//! (the scope's file blobs and their elements, its conversations' turns, the
//! summaries of both). On prod (2026-10-15) that was 0.36 s for a 13k-source
//! code scope and 2.5 s cold for a 220k one, on every pass of every search.
//! [`cached_scope`] keeps the collected set in `scope_sources`, and the search
//! statement reads it plus what grew since it was built.
//!
//! **Never missing a source** is the whole contract, because a source the
//! scope lacks is content the search cannot find. Migration 0032 stamps
//! `changed_xid` - the writing transaction's xid - by trigger on every write
//! that can grow a scope (a new or rewritten element, a path added or
//! re-pointed, a summary, a conversation's new turns or new anchor). A cached
//! scope's cutoff is the xmin of the snapshot it was built from, and its
//! delta reads every row stamped at or after it: every transaction that
//! snapshot could not see. That is MVCC's own visibility rule, so a write the
//! build missed - in flight, or not yet begun - is always read.
//!
//! **Shrinking is bounded, not immediate.** A cached scope is a superset of
//! the true one, and every candidate is re-checked against live provenance at
//! admission. The sources that left each repository's scopes are counted
//! (`scope_shrinks`), and a key is rebuilt before they reach
//! [`SCOPE_SHRINK_REBUILD`], or after [`SCOPE_CACHE_MAX_AGE`].

use std::time::Duration;

use crate::{PgPool, SearchFilters, StoreError};

/// How long a cached scope serves before it is rebuilt.
pub const SCOPE_CACHE_MAX_AGE: Duration = Duration::from_secs(600);

/// The most departed sources a cached scope may still carry: it is rebuilt
/// once the sources that left its repositories since it was built reach this.
///
/// A departed source costs nothing in correctness (admission drops it), but
/// it does two things a live one would not. It takes a candidate slot, so a
/// cached page could need a pass more than the collected one. And it counts
/// toward the scope's size, which picks exact or approximate ranking: so a
/// search of a cached scope ranks exactly anything within this many sources
/// above a limit (the departures since its cutoff, summed by the search
/// statement, plus any late summary of a departed source), where the true
/// count may be under it. Departures are logged as upper bounds - a raw hash
/// and every summary it has - so the cache never carries more than they say.
///
/// 2,000 lets ordinary editing run: a saved file's old blob is a few dozen
/// sources, so a key is rebuilt every fifty or so saves rather than on each,
/// and a branch switch rebuilds it at once. It is 1.3% of the 150k exact
/// limit, the band the exact ranking above may widen by.
pub const SCOPE_SHRINK_REBUILD: i64 = 2_000;

/// The least time between two builds of one key.
///
/// A key that has lost [`SCOPE_SHRINK_REBUILD`] sources within this long of
/// its last build is collected in full rather than rebuilt. A build costs
/// several times a plain collection (it writes every source back), so under
/// heavy churn - a reparse wave after a parser bump, ~1M elements - rebuilding
/// on every threshold would make each search slower than no cache at all.
/// With this, the worst a search pays is the uncached collection, and each
/// key is rebuilt at most once a minute.
pub const SCOPE_REBUILD_COOLDOWN: Duration = Duration::from_secs(60);

/// A key not rebuilt for this long has not been searched; the next build
/// anywhere drops it.
const SCOPE_CACHE_IDLE: Duration = Duration::from_secs(3_600);

/// How a search came by its scope.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScopeCacheUse {
    /// Collected in full: the cache is off, the search is unscoped, or its
    /// filters are ones the cache does not key (a conversation, a ddoc field).
    #[default]
    Bypassed,
    /// Read from a cached scope plus what grew since.
    Hit,
    /// Collected in full into the cache (missing, old, or too much had left
    /// it), then read from it.
    Built,
    /// Another search was building this scope; collected in full rather than
    /// waiting.
    Busy,
    /// Too much had left the cached scope, and it was built less than
    /// [`SCOPE_REBUILD_COOLDOWN`] ago: collected in full rather than rebuilt.
    Cooling,
}

impl ScopeCacheUse {
    /// The spelling `meta.timing` reports.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Bypassed => "bypassed",
            Self::Hit => "hit",
            Self::Built => "built",
            Self::Busy => "busy",
            Self::Cooling => "cooling",
        }
    }
}

/// A cached scope a search reads: its key, and the cutoff its delta reads
/// from (an xid, as Postgres spells an `xid8`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CachedScope {
    /// The `scope_cache` key.
    pub key: String,
    /// Rows stamped by this transaction or a later one are read on top of
    /// the cached set.
    pub cutoff: String,
}

/// The cache key for `filters`' scope, or `None` when the cache does not
/// serve it.
///
/// The key covers every input of the scope collection the cache serves:
/// repository, worktree, path, kinds, and the parser version that answers
/// for a blob. A conversation scope is small and
/// ranked exactly anyway, and a ddoc filter (`id_kinds`, `ddoc_schema`) reads
/// element fields that are rewritten in place without a stamp, so both are
/// collected in full.
#[must_use]
pub fn scope_key(filters: &SearchFilters) -> Option<String> {
    if !filters.scope_cache
        || (filters.repo.is_none() && filters.path.is_none() && filters.worktree.is_none())
        || filters.conversation.is_some()
        || filters.id_kinds.is_some()
        || filters.ddoc_schema.is_some()
    {
        return None;
    }
    let mut kinds: Option<Vec<&str>> = filters
        .kinds
        .as_ref()
        .map(|kinds| kinds.iter().map(|kind| kind.as_str()).collect());
    if let Some(kinds) = kinds.as_mut() {
        kinds.sort_unstable();
        kinds.dedup();
    }
    let canonical = format!(
        "scope-v2\n{:?}\n{:?}\n{:?}\n{:?}\n{:?}",
        filters.repo, filters.worktree, filters.path, kinds, filters.parser_version
    );
    Some(fs3_core::content_hash(canonical.as_bytes()))
}

/// The repositories whose departures make `filters`' cached scope stale, or
/// `None` for every repository.
///
/// A scope named by its repository holds that repository's files and
/// conversations. Without one, a worktree scope also holds every conversation
/// with no worktree, whatever its repository, and a path scope spans them
/// all. `*` counts departures no repository can be named for.
fn watched(filters: &SearchFilters) -> Option<Vec<String>> {
    filters
        .repo
        .as_ref()
        .map(|repo| vec![repo.clone(), "*".to_string()])
}

/// The cached scope a search of `filters` should read, building it when it is
/// missing, older than [`SCOPE_CACHE_MAX_AGE`], or has lost
/// [`SCOPE_SHRINK_REBUILD`] sources since it was built.
///
/// Returns the scope to bind (`None`: collect the scope in full) and how it
/// was come by. A cache that fails is logged and bypassed: the search is
/// answered either way.
pub async fn cached_scope(
    pool: &PgPool,
    filters: &SearchFilters,
) -> (Option<CachedScope>, ScopeCacheUse) {
    let Some(key) = scope_key(filters) else {
        return (None, ScopeCacheUse::Bypassed);
    };
    match fresh_or_build(pool, &key, filters).await {
        Ok((used, Some(cutoff))) => (Some(CachedScope { key, cutoff }), used),
        Ok((used, None)) => (None, used),
        Err(error) => {
            tracing::warn!(%error, "scope cache unavailable; collecting the scope in full");
            (None, ScopeCacheUse::Bypassed)
        }
    }
}

/// How the scope is come by and, when cached, the cutoff to read it with.
async fn fresh_or_build(
    pool: &PgPool,
    key: &str,
    filters: &SearchFilters,
) -> Result<(ScopeCacheUse, Option<String>), StoreError> {
    // (cutoff, departed since, young enough, built within the cooldown)
    let cached: Option<(String, i64, bool, bool)> = sqlx::query_as(
        "SELECT c.cutoff::text,
                (SELECT COALESCE(sum(s.removed), 0)::bigint
                   FROM scope_shrinks s
                  WHERE (c.watch IS NULL OR s.identity = ANY(c.watch))
                    AND s.xid >= c.cutoff),
                c.built_at > now() - make_interval(secs => $2),
                c.built_at > now() - make_interval(secs => $3)
           FROM scope_cache c
          WHERE c.scope_key = $1",
    )
    .bind(key)
    .bind(SCOPE_CACHE_MAX_AGE.as_secs_f64())
    .bind(SCOPE_REBUILD_COOLDOWN.as_secs_f64())
    .fetch_optional(pool)
    .await?;
    match cached {
        Some((cutoff, departed, true, _)) if departed < SCOPE_SHRINK_REBUILD => {
            return Ok((ScopeCacheUse::Hit, Some(cutoff)));
        }
        Some((_, _, true, true)) => return Ok((ScopeCacheUse::Cooling, None)),
        _ => {}
    }
    Ok(match build(pool, key, filters).await? {
        Some(cutoff) => (ScopeCacheUse::Built, Some(cutoff)),
        None => (ScopeCacheUse::Busy, None),
    })
}

/// Collect `filters`' scope in full into the cache under `key`.
///
/// The cutoff it was built at, or `None` when another search holds this
/// key's build: the caller searches uncached rather than wait.
async fn build(
    pool: &PgPool,
    key: &str,
    filters: &SearchFilters,
) -> Result<Option<String>, StoreError> {
    let watch = watched(filters);
    let mut tx = pool.begin().await?;
    // One snapshot for everything below: the collection, its cutoff, and the
    // departures already counted.
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ")
        .execute(&mut *tx)
        .await?;
    let locked: bool =
        sqlx::query_scalar("SELECT pg_try_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(key)
            .fetch_one(&mut *tx)
            .await?;
    if !locked {
        tx.rollback().await?;
        return Ok(None);
    }
    // The oldest transaction this snapshot could not see: everything it
    // missed was written by that one or a later one, departures included.
    let cutoff: String = sqlx::query_scalar("SELECT pg_snapshot_xmin(pg_current_snapshot())::text")
        .fetch_one(&mut *tx)
        .await?;

    // Gone first, so the collection below sees no cached scope for this key
    // and collects it in full.
    sqlx::query("DELETE FROM scope_cache WHERE scope_key = $1")
        .bind(key)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM scope_sources WHERE scope_key = $1")
        .bind(key)
        .execute(&mut *tx)
        .await?;
    let sources = sqlx::query(BUILD_SQL)
        // Binds line up with SEARCH_ELEMENTS_SQL's; only the scope's are read.
        .bind(Option::<&str>::None)
        .bind(Option::<&str>::None)
        .bind(Option::<i64>::None)
        .bind(Option::<&str>::None)
        .bind(Option::<f64>::None)
        .bind(filters.repo.as_deref())
        .bind(filters.path.as_deref())
        .bind(
            filters
                .kinds
                .as_ref()
                .map(|kinds| kinds.iter().map(|kind| kind.as_str()).collect::<Vec<_>>()),
        )
        .bind(filters.worktree.as_deref())
        .bind(Option::<Vec<String>>::None)
        .bind(Option::<bool>::None)
        .bind(Option::<&str>::None)
        .bind(Option::<&str>::None)
        .bind(Option::<i64>::None)
        .bind(Option::<Vec<String>>::None)
        .bind(Option::<i64>::None)
        .bind(Option::<i64>::None)
        .bind(filters.parser_version.as_deref())
        .bind(key)
        .bind(Option::<&str>::None)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    sqlx::query(
        "INSERT INTO scope_cache (scope_key, cutoff, built_at, sources, watch)
         VALUES ($1, $2::xid8, now(), $3, $4)",
    )
    .bind(key)
    .bind(&cutoff)
    .bind(i64::try_from(sources).unwrap_or(i64::MAX))
    .bind(watch.as_deref())
    .execute(&mut *tx)
    .await?;

    // Scopes nobody has searched for a while.
    sqlx::query(
        "WITH idle AS (
             DELETE FROM scope_cache
              WHERE built_at < now() - make_interval(secs => $1)
             RETURNING scope_key
         )
         DELETE FROM scope_sources s USING idle WHERE s.scope_key = idle.scope_key",
    )
    .bind(SCOPE_CACHE_IDLE.as_secs_f64())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    // After the commit and in a statement of its own, so that two builds
    // overlapping never conflict over the same entries (a serialisation
    // failure would throw a whole build away). Only entries older than any
    // key may live are pruned, and only those every committed key has passed:
    // a build still in flight with an earlier cutoff is invisible here, but
    // the entries it could miss are younger than this. A failed prune is
    // retried by the next build.
    if let Err(error) = sqlx::query(
        "DELETE FROM scope_shrinks
          WHERE logged_at < now() - make_interval(secs => $1)
            AND xid < (SELECT min(cutoff) FROM scope_cache)",
    )
    .bind(SCOPE_CACHE_MAX_AGE.as_secs_f64())
    .execute(pool)
    .await
    {
        tracing::warn!(%error, "could not prune the scope departure log");
    }
    Ok(Some(cutoff))
}

/// Empty the cache, as boot does after migrating: a schema change can change
/// what a scope holds, and no cached scope should outlive one.
///
/// # Errors
/// [`StoreError::Query`] when the statement fails.
pub async fn reset_scope_cache(pool: &PgPool) -> Result<(), StoreError> {
    sqlx::query("TRUNCATE scope_sources, scope_cache, scope_shrinks")
        .execute(pool)
        .await?;
    Ok(())
}

const BUILD_SQL: &str = concat!(
    "WITH ",
    crate::embeddings::scope_ctes!(),
    "
INSERT INTO scope_sources (scope_key, source_kind, source_hash)
SELECT $19, source_kind, source_hash FROM admitted_sources"
);
