//! The schema gate every db-touching endpoint runs first (tk-0108).
//!
//! One indexed read, and it changes what a stale database FEELS like. Without
//! it, a daemon whose binary carries migration 0007 against a database at 0005
//! answers `column "enrich" does not exist` — a true statement that requires the
//! reader to know the migration history to translate. With it, the answer is
//! "the schema is 0006-0007 behind; run `flowspace3 doctor`", which is the whole
//! actionable-error doctrine in one line.
//!
//! # Why the daemon needs this at all, given it migrates at boot
//!
//! Boot covers the common case and cannot cover the interesting one: a database
//! restored from a backup, pointed at a different instance by an `FS3_*`
//! override, or shared with a newer daemon that a colleague ran. In every one of
//! those the process is already up, so boot has been and gone. The guard is the
//! per-request answer to "is the thing I am about to write to still the thing I
//! migrated?".
//!
//! # Why it is cheap enough to run per request
//!
//! `schema_current` is one `to_regclass` and one indexed read of a table with as
//! many rows as fs3 has migrations. Caching it would be faster and wrong: the
//! failure this exists to catch is precisely the schema changing underneath a
//! running process, and a cache would hide exactly that.

use fs3_core::catalog;
use fs3_core::envelope::Failure;
use fs3_store::PgPool;

use crate::answer::IntoFailure;

/// Refuse to work against a database that is behind this binary.
///
/// # Errors
/// [`catalog::STORE_SCHEMA_STALE`] naming the missing migrations and pointing at
/// doctor; [`catalog::STORE_DATABASE_MISSING`] when the database is not there at
/// all; [`catalog::STORE_UNAVAILABLE`] when the server is not.
pub async fn guard(pool: &PgPool) -> Result<(), Failure> {
    let status = fs3_store::schema_current(pool)
        .await
        .map_err(IntoFailure::into_failure)?;

    if status.is_current() {
        return Ok(());
    }

    Err(Failure::new(
        &catalog::STORE_SCHEMA_STALE,
        format!(
            "the database is missing migration(s) {} — this binary carries {} and the \
                 database has applied {}",
            status.missing_summary(),
            status.embedded.len(),
            status.applied.len()
        ),
    )
    .with_detail("missing", status.missing.clone())
    .with_detail("applied", status.applied.len())
    .with_detail("embedded", status.embedded.len()))
}

/// A database AHEAD of this binary, for a status line.
///
/// Not a refusal: a newer daemon migrated it and this one's queries may be
/// perfectly fine. Reported rather than hidden, because it explains a column
/// nobody here expects and it is the first thing to check when two daemons
/// disagree.
pub async fn ahead_of_us(pool: &PgPool) -> Vec<i64> {
    fs3_store::schema_current(pool)
        .await
        .map(|status| status.ahead())
        .unwrap_or_default()
}
