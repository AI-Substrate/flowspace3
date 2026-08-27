//! The user messages queue: push, carry, clear (PRD req 59).
//!
//! Three functions, because the queue has exactly three flows:
//!
//! * [`sync_messages`] — a producer declares what its source should be saying
//!   right now. Level-triggered: what it does not declare, it retracts.
//! * [`live_messages`] — what every daemon envelope carries.
//! * [`ack_message`] — a human waving one away.
//!
//! # Ownership is (source, install path), not source alone
//!
//! One source, one producer was the original rule, and it is what makes the
//! delete half of [`sync_messages`] safe. It turned out to be one dimension
//! short: a store is shared by every INSTALLATION pointed at it, so "the
//! update producer" is not one producer — it is one per install path, and
//! they were retracting and overwriting each other's rows. Root's daemon
//! carried another user's "not writable" message about a path root does not
//! use (Jordan ruled per-install update truth, 2026-08-27).
//!
//! So a scope rides beside the source. `None` means "concerns every
//! installation on this store", which is what a schema skew or an unwritable
//! log directory genuinely is; `Some(path)` means "concerns the install at
//! this path, and nobody else". Both halves respect it: a declaration retracts
//! only its own scope, and a reader sees only its own scope plus the
//! everyone-messages. Splitting the update STATE row without this would have
//! fixed nothing a user could see, because [`live_messages`] returns rows, not
//! rows-for-you.
//!
//! Timestamps stay server-side. Nothing here reads a `TIMESTAMPTZ` into Rust:
//! `created` crosses the wire as text formatted by Postgres, so the store keeps
//! sqlx's date-time features off and two daemons on two machines cannot
//! disagree about what "now" was.

use fs3_core::messages::{Severity, UserMessage};
use sqlx::Row;

use crate::{PgPool, StoreError};

/// Postgres' spelling of RFC 3339 in UTC, so the wire format is decided here
/// once rather than by whatever formatter a caller reaches for.
const CREATED_AS_TEXT: &str =
    "to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created";

/// Make `source`'s messages within `install_path` exactly `desired`, in one
/// transaction.
///
/// This is the whole clearing story. A producer that has nothing to say passes
/// an empty slice and its previous messages disappear; a producer that still
/// has the same thing to say passes the same key and nothing observable
/// changes. The update supervisor calls this on every reconcile pass, which is
/// why "the update succeeded, so its message is gone" needs no code of its own.
///
/// `install_path` is the scope this declaration speaks for: `None` for a
/// producer whose news concerns every installation on the store (schema skew,
/// logging), `Some(path)` for one that speaks only for one install. A
/// declaration NEVER retracts another scope's rows, which is why one daemon
/// re-declaring its own two messages cannot silently delete the four another
/// install is standing on.
///
/// Re-pushing a key the user has ACKED leaves the ack alone: waving a message
/// away must not be undone by the next pass of the loop that raised it. A
/// producer that genuinely has new news gives it a new key — which is why keys
/// carry the version they are about, and, for a scoped producer, the install
/// they are about: `key` is the PRIMARY KEY, so two installs sharing a key
/// would be one row they take turns overwriting.
///
/// # Errors
/// [`StoreError::Query`] when a statement fails; the transaction rolls back, so
/// a source is never left half-declared.
pub async fn sync_messages(
    pool: &PgPool,
    source: &str,
    install_path: Option<&str>,
    desired: &[UserMessage],
) -> Result<(), StoreError> {
    let mut transaction = pool.begin().await?;

    let keys: Vec<&str> = desired.iter().map(|message| message.key.as_str()).collect();

    // Retract first: a key that moved out of the desired set is gone before
    // anything is written, so a producer can rename a message in one pass.
    //
    // `IS NOT DISTINCT FROM` rather than `=` because the scope is nullable and
    // `install_path = NULL` is never true: a global producer re-declaring would
    // otherwise retract nothing at all and its messages would accumulate
    // forever.
    sqlx::query(
        "DELETE FROM user_messages
          WHERE source = $1
            AND install_path IS NOT DISTINCT FROM $2
            AND key <> ALL($3)",
    )
    .bind(source)
    .bind(install_path)
    .bind(&keys)
    .execute(&mut *transaction)
    .await?;

    for message in desired {
        sqlx::query(
            "INSERT INTO user_messages
                 (key, source, install_path, severity, text, next_action, expires_at)
             VALUES ($1, $2, $3, $4, $5, $6, NULL)
             ON CONFLICT (key) DO UPDATE SET
               source       = EXCLUDED.source,
               install_path = EXCLUDED.install_path,
               severity     = EXCLUDED.severity,
               text         = EXCLUDED.text,
               next_action  = EXCLUDED.next_action,
               updated_at   = now()",
        )
        .bind(&message.key)
        .bind(source)
        .bind(install_path)
        .bind(message.severity.as_str())
        .bind(&message.text)
        .bind(&message.next_action)
        .execute(&mut *transaction)
        .await?;
    }

    transaction.commit().await?;
    Ok(())
}

/// Every message the installation at `install_path` has not dismissed and that
/// has not expired, oldest first.
///
/// Oldest first because a standing condition outranks fresh news: the reason
/// the install is broken should be read before the notice that something new
/// arrived on top of it.
///
/// Scoped, and this is the half of the per-install fix a user can actually
/// see. A row scoped to another install is not merely someone else's — it is
/// UNACTIONABLE here, and `next_action` is NOT NULL precisely so that cannot
/// happen. Rows with no scope concern every installation and are carried by
/// all of them.
///
/// The consequence, named rather than hidden: an envelope carries the messages
/// of the installation that BUILT it. A CLI at one path talking to a daemon at
/// another sees the daemon's, because the answer came from the daemon — and
/// `flowspace3 doctor` holds its own pool, so it is the verb that speaks for
/// YOUR install. Carrying both would mean the CLI telling the daemon its path
/// on the wire, which is a protocol change and not this packet's.
///
/// # Errors
/// [`StoreError::Query`] when the statement fails.
pub async fn live_messages(
    pool: &PgPool,
    install_path: &str,
) -> Result<Vec<UserMessage>, StoreError> {
    let rows = sqlx::query(&format!(
        "SELECT key, source, severity, text, next_action, {CREATED_AS_TEXT}
           FROM user_messages
          WHERE acked_at IS NULL
            AND (expires_at IS NULL OR expires_at > now())
            AND (install_path IS NULL OR install_path = $1)
          ORDER BY created_at, key"
    ))
    .bind(install_path)
    .fetch_all(pool)
    .await?;

    rows.into_iter().map(read_message).collect()
}

/// Dismiss one message by key. Returns whether there was one to dismiss.
///
/// # Errors
/// [`StoreError::Query`] when the statement fails.
pub async fn ack_message(pool: &PgPool, key: &str) -> Result<bool, StoreError> {
    let done = sqlx::query(
        "UPDATE user_messages SET acked_at = now() WHERE key = $1 AND acked_at IS NULL",
    )
    .bind(key)
    .execute(pool)
    .await?;

    Ok(done.rows_affected() > 0)
}

fn read_message(row: sqlx::postgres::PgRow) -> Result<UserMessage, StoreError> {
    let severity: String = row.try_get("severity")?;
    Ok(UserMessage {
        key: row.try_get("key")?,
        source: row.try_get("source")?,
        // The CHECK constraint already refuses anything else, so an unknown
        // spelling here means the schema was edited by hand. Say so rather than
        // silently downgrading it to `info`.
        severity: Severity::parse(&severity).ok_or_else(|| {
            StoreError::Corrupt(fs3_core::Error::InvalidConfig(format!(
                "user_messages.severity = {severity:?}, which is not one of info/warning/error"
            )))
        })?,
        text: row.try_get("text")?,
        next_action: row.try_get("next_action")?,
        created: row.try_get("created")?,
    })
}
