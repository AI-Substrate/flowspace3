//! Persistence for the auto-updater's one row (PRD req 54).
//!
//! The decisions live in [`fs3_core::update`]; this module only reads and
//! writes them. Three flows:
//!
//! * [`claim_check`] — "is it my turn to ask GitHub?", answered by the database
//!   rather than by a timer, so two daemons on one store cannot both win.
//! * [`record_seen`] / [`record_installed`] / [`record_blocked`] /
//!   [`record_clear`] — what the last pass concluded.
//! * [`update_state`] — what doctor and the message producer read back.
//!
//! Timestamps stay server-side, as everywhere else in this crate.

use fs3_core::update::UpdateState;
use sqlx::Row;

use crate::{PgPool, StoreError};

const STATE_COLUMNS: &str = "latest_seen, installed_version, install_path, blocked_reason, \
     to_char(last_checked_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS last_checked";

/// Take the right to run a release check, if one is due.
///
/// Returns `true` when this caller won the claim, stamping `last_checked_at` in
/// the same statement. The stamp lands on the CLAIM rather than on a successful
/// probe deliberately: a probe that fails must not become a retry every few
/// seconds against a rate-limited endpoint (fleet retro DL-018). A failed check
/// waits out the interval, which is what the reconcile loop's "the next pass is
/// the recovery mechanism" contract already promises.
///
/// # Errors
/// [`StoreError::Query`] when the statement fails.
pub async fn claim_check(pool: &PgPool, interval_hours: u64) -> Result<bool, StoreError> {
    let claimed = sqlx::query(
        "UPDATE update_state
            SET last_checked_at = now()
          WHERE singleton
            AND (last_checked_at IS NULL
                 OR last_checked_at < now() - make_interval(hours => $1))",
    )
    .bind(i32::try_from(interval_hours).unwrap_or(i32::MAX))
    .execute(pool)
    .await?;

    Ok(claimed.rows_affected() > 0)
}

/// Read the whole of it.
///
/// # Errors
/// [`StoreError::Query`] when the statement fails.
pub async fn update_state(pool: &PgPool) -> Result<UpdateState, StoreError> {
    let row = sqlx::query(&format!(
        "SELECT {STATE_COLUMNS} FROM update_state WHERE singleton"
    ))
    .fetch_one(pool)
    .await?;

    Ok(UpdateState {
        latest_seen: row.try_get("latest_seen")?,
        installed_version: row.try_get("installed_version")?,
        install_path: row.try_get("install_path")?,
        blocked_reason: row.try_get("blocked_reason")?,
        last_checked: row.try_get("last_checked")?,
    })
}

/// Record that the probe saw `latest`, whatever came of it.
///
/// # Errors
/// [`StoreError::Query`] when the statement fails.
pub async fn record_seen(pool: &PgPool, latest: &str) -> Result<(), StoreError> {
    sqlx::query("UPDATE update_state SET latest_seen = $1 WHERE singleton")
        .bind(latest)
        .execute(pool)
        .await?;
    Ok(())
}

/// Record a completed atomic swap, clearing whatever was blocking before it.
///
/// # Errors
/// [`StoreError::Query`] when the statement fails.
pub async fn record_installed(
    pool: &PgPool,
    version: &str,
    install_path: &str,
) -> Result<(), StoreError> {
    sqlx::query(
        "UPDATE update_state
            SET installed_version = $1,
                install_path      = $2,
                installed_at      = now(),
                latest_seen       = $1,
                blocked_reason    = NULL
          WHERE singleton",
    )
    .bind(version)
    .bind(install_path)
    .execute(pool)
    .await?;
    Ok(())
}

/// Record why an update could not be installed, and where.
///
/// # Errors
/// [`StoreError::Query`] when the statement fails.
pub async fn record_blocked(
    pool: &PgPool,
    reason: &str,
    install_path: &str,
) -> Result<(), StoreError> {
    sqlx::query("UPDATE update_state SET blocked_reason = $1, install_path = $2 WHERE singleton")
        .bind(reason)
        .bind(install_path)
        .execute(pool)
        .await?;
    Ok(())
}

/// Clear a previous block without claiming an install — the "we looked, we are
/// already current" outcome.
///
/// # Errors
/// [`StoreError::Query`] when the statement fails.
pub async fn record_clear(pool: &PgPool) -> Result<(), StoreError> {
    sqlx::query("UPDATE update_state SET blocked_reason = NULL WHERE singleton")
        .execute(pool)
        .await?;
    Ok(())
}
