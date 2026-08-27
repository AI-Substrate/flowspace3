//! Persistence for the auto-updater's state, ONE ROW PER INSTALL PATH (PRD
//! req 54).
//!
//! The decisions live in [`fs3_core::update`]; this module only reads and
//! writes them. Three flows:
//!
//! * [`claim_check`] / [`claim_check_now`] — "is it my turn to ask GitHub?",
//!   answered by the database rather than by a timer, so two daemons on one
//!   install cannot both win.
//! * [`record_seen`] / [`record_swapped`] / [`record_on_disk`] /
//!   [`record_blocked`] / [`record_clear`] — what the last pass concluded.
//! * [`update_state`] — what doctor and the message producer read back.
//!
//! # Every function takes an install path, and that is the point
//!
//! The row used to be a singleton per store, and a store is shared by every
//! installation pointed at it. Two installs — which `install.sh` produces on
//! its own, choosing `/usr/local/bin` or `~/.local/bin` by permission —
//! thrashed that one row last-writer-wins, so one install carried the other's
//! blocked message about a path it does not use. The path is the identity now,
//! so a caller that cannot name an install cannot write update state at all.
//!
//! # Every writer is an upsert, because there is no seed row
//!
//! 0009 seeded its singleton so every later statement could be an UPDATE.
//! There is nothing to seed here: the set of install paths pointed at a store
//! is discovered, not declared. `doctor upgrade` in particular writes without
//! ever claiming an interval, so a plain UPDATE would silently no-op on the
//! first run of a fresh install — the worst possible failure, because it looks
//! exactly like success.
//!
//! Timestamps stay server-side, as everywhere else in this crate.

use fs3_core::update::UpdateState;
use sqlx::Row;

use crate::{PgPool, StoreError};

const STATE_COLUMNS: &str = "install_path, latest_seen, installed_version, blocked_reason, \
     to_char(last_checked_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS last_checked";

/// Take the right to run a release check for `install_path`, if one is due.
///
/// Returns `true` when this caller won the claim, stamping `last_checked_at` in
/// the same statement. The stamp lands on the CLAIM rather than on a successful
/// probe deliberately: a probe that fails must not become a retry every few
/// seconds against a rate-limited endpoint (fleet retro DL-018). A failed check
/// waits out the interval, which is what the reconcile loop's "the next pass is
/// the recovery mechanism" contract already promises.
///
/// The interval is now honoured PER INSTALL. That is the correct reading of
/// DL-018 rather than merely the consistent one: two installs are two things to
/// keep current, and one starving the other of checks was never the intent. The
/// cost to a fleet is unchanged — one probe per install per interval.
///
/// # Errors
/// [`StoreError::Query`] when the statement fails.
pub async fn claim_check(
    pool: &PgPool,
    install_path: &str,
    interval_hours: u64,
) -> Result<bool, StoreError> {
    // An install that has never been seen inserts, and inserting IS the claim:
    // a brand-new install must check immediately rather than wait out an
    // interval it has never once served.
    let claimed = sqlx::query(
        "INSERT INTO update_state (install_path, last_checked_at)
              VALUES ($1, now())
         ON CONFLICT (install_path) DO UPDATE
                 SET last_checked_at = now()
               WHERE update_state.last_checked_at IS NULL
                  OR update_state.last_checked_at < now() - make_interval(hours => $2)",
    )
    .bind(install_path)
    .bind(i32::try_from(interval_hours).unwrap_or(i32::MAX))
    .execute(pool)
    .await?;

    Ok(claimed.rows_affected() > 0)
}

/// Claim a check for `install_path` unconditionally — the boot check.
///
/// A standing message is level-triggered, but the level was only ever re-read
/// on the producer's cadence, and boot did not tick it. So a daemon that
/// restarted onto a current binary went on carrying a message written a day
/// earlier by a process that no longer exists: nothing re-evaluated, so nothing
/// retracted. Every boot claims once, and the pass that follows either
/// refreshes or retracts within seconds.
///
/// Unconditional, so two daemons booting on one install path at the same moment
/// both probe. That is one extra request bounded by how often daemons boot, and
/// it is the trade the ruling asks for: the alternative is a booting daemon
/// silently inheriting a claim it cannot verify.
///
/// # Errors
/// [`StoreError::Query`] when the statement fails.
pub async fn claim_check_now(pool: &PgPool, install_path: &str) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO update_state (install_path, last_checked_at)
              VALUES ($1, now())
         ON CONFLICT (install_path) DO UPDATE SET last_checked_at = now()",
    )
    .bind(install_path)
    .execute(pool)
    .await?;
    Ok(())
}

/// Read the whole of one installation's state.
///
/// An install nobody has ever checked has no row, and that is a state rather
/// than an error: a default carrying its own path, which declares no messages.
///
/// # Errors
/// [`StoreError::Query`] when the statement fails.
pub async fn update_state(pool: &PgPool, install_path: &str) -> Result<UpdateState, StoreError> {
    let row = sqlx::query(&format!(
        "SELECT {STATE_COLUMNS} FROM update_state WHERE install_path = $1"
    ))
    .bind(install_path)
    .fetch_optional(pool)
    .await?;

    let Some(row) = row else {
        return Ok(UpdateState {
            install_path: install_path.to_string(),
            ..UpdateState::default()
        });
    };

    Ok(UpdateState {
        install_path: row.try_get("install_path")?,
        latest_seen: row.try_get("latest_seen")?,
        installed_version: row.try_get("installed_version")?,
        blocked_reason: row.try_get("blocked_reason")?,
        last_checked: row.try_get("last_checked")?,
    })
}

/// Record that the probe saw `latest`, whatever came of it.
///
/// # Errors
/// [`StoreError::Query`] when the statement fails.
pub async fn record_seen(
    pool: &PgPool,
    install_path: &str,
    latest: &str,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO update_state (install_path, latest_seen)
              VALUES ($1, $2)
         ON CONFLICT (install_path) DO UPDATE SET latest_seen = EXCLUDED.latest_seen",
    )
    .bind(install_path)
    .bind(latest)
    .execute(pool)
    .await?;
    Ok(())
}

/// Record that a swap landed, clearing whatever was blocking before it.
///
/// Note what this does NOT write: `installed_version`. What is installed is
/// read from the file by [`record_on_disk`], and having exactly one writer for
/// that fact is the whole of the disk-reconciliation fix. A swap that succeeded
/// and a reinstall someone did behind the daemon's back then produce the same
/// answer, because both are read from the same place.
///
/// # Errors
/// [`StoreError::Query`] when the statement fails.
pub async fn record_swapped(
    pool: &PgPool,
    install_path: &str,
    version: &str,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO update_state (install_path, latest_seen, installed_at, blocked_reason)
              VALUES ($1, $2, now(), NULL)
         ON CONFLICT (install_path) DO UPDATE
                 SET latest_seen    = EXCLUDED.latest_seen,
                     installed_at   = now(),
                     blocked_reason = NULL",
    )
    .bind(install_path)
    .bind(version)
    .execute(pool)
    .await?;
    Ok(())
}

/// Record what the binary at `install_path` actually reports itself to be, or
/// `None` when there is nothing there that can be asked.
///
/// The only writer of `installed_version`, and the reason the row can no longer
/// drift away from the disk it describes. `None` is a real answer and must be
/// written as one: it is how "somebody deleted the binary" and "the path holds
/// something that will not run" retract a standing restart message instead of
/// leaving it to outlive its cause.
///
/// # Errors
/// [`StoreError::Query`] when the statement fails.
pub async fn record_on_disk(
    pool: &PgPool,
    install_path: &str,
    version: Option<&str>,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO update_state (install_path, installed_version)
              VALUES ($1, $2)
         ON CONFLICT (install_path) DO UPDATE SET installed_version = EXCLUDED.installed_version",
    )
    .bind(install_path)
    .bind(version)
    .execute(pool)
    .await?;
    Ok(())
}

/// Record why an update could not be installed at `install_path`.
///
/// # Errors
/// [`StoreError::Query`] when the statement fails.
pub async fn record_blocked(
    pool: &PgPool,
    install_path: &str,
    reason: &str,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO update_state (install_path, blocked_reason)
              VALUES ($1, $2)
         ON CONFLICT (install_path) DO UPDATE SET blocked_reason = EXCLUDED.blocked_reason",
    )
    .bind(install_path)
    .bind(reason)
    .execute(pool)
    .await?;
    Ok(())
}

/// Clear a previous block without claiming an install — the "we looked, we are
/// already current" outcome.
///
/// # Errors
/// [`StoreError::Query`] when the statement fails.
pub async fn record_clear(pool: &PgPool, install_path: &str) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO update_state (install_path)
              VALUES ($1)
         ON CONFLICT (install_path) DO UPDATE SET blocked_reason = NULL",
    )
    .bind(install_path)
    .execute(pool)
    .await?;
    Ok(())
}
