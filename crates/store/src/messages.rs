//! The user messages queue: push, carry, clear (PRD req 59).
//!
//! Three functions, because the queue has exactly three flows:
//!
//! * [`sync_messages`] — a producer declares what its source should be saying
//!   right now. Level-triggered: what it does not declare, it retracts.
//! * [`live_messages`] — what every daemon envelope carries.
//! * [`ack_message`] — a human waving one away.
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

/// Make `source`'s messages exactly `desired`, in one transaction.
///
/// This is the whole clearing story. A producer that has nothing to say passes
/// an empty slice and its previous messages disappear; a producer that still
/// has the same thing to say passes the same key and nothing observable
/// changes. The update supervisor calls this on every reconcile pass, which is
/// why "the update succeeded, so its message is gone" needs no code of its own.
///
/// Re-pushing a key the user has ACKED leaves the ack alone: waving a message
/// away must not be undone by the next pass of the loop that raised it. A
/// producer that genuinely has new news gives it a new key — which is why keys
/// carry the version they are about.
///
/// # Errors
/// [`StoreError::Query`] when a statement fails; the transaction rolls back, so
/// a source is never left half-declared.
pub async fn sync_messages(
    pool: &PgPool,
    source: &str,
    desired: &[UserMessage],
) -> Result<(), StoreError> {
    let mut transaction = pool.begin().await?;

    let keys: Vec<&str> = desired.iter().map(|message| message.key.as_str()).collect();

    // Retract first: a key that moved out of the desired set is gone before
    // anything is written, so a producer can rename a message in one pass.
    sqlx::query("DELETE FROM user_messages WHERE source = $1 AND key <> ALL($2)")
        .bind(source)
        .bind(&keys)
        .execute(&mut *transaction)
        .await?;

    for message in desired {
        sqlx::query(
            "INSERT INTO user_messages (key, source, severity, text, next_action, expires_at)
             VALUES ($1, $2, $3, $4, $5, NULL)
             ON CONFLICT (key) DO UPDATE SET
               source      = EXCLUDED.source,
               severity    = EXCLUDED.severity,
               text        = EXCLUDED.text,
               next_action = EXCLUDED.next_action,
               updated_at  = now()",
        )
        .bind(&message.key)
        .bind(source)
        .bind(message.severity.as_str())
        .bind(&message.text)
        .bind(&message.next_action)
        .execute(&mut *transaction)
        .await?;
    }

    transaction.commit().await?;
    Ok(())
}

/// Every message a user has not dismissed and that has not expired, oldest
/// first.
///
/// Oldest first because a standing condition outranks fresh news: the reason
/// the install is broken should be read before the notice that something new
/// arrived on top of it.
///
/// # Errors
/// [`StoreError::Query`] when the statement fails.
pub async fn live_messages(pool: &PgPool) -> Result<Vec<UserMessage>, StoreError> {
    let rows = sqlx::query(&format!(
        "SELECT key, source, severity, text, next_action, {CREATED_AS_TEXT}
           FROM user_messages
          WHERE acked_at IS NULL
            AND (expires_at IS NULL OR expires_at > now())
          ORDER BY created_at, key"
    ))
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
