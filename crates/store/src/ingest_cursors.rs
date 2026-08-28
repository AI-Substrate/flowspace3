//! Where a native-store read stopped, and what it has already stored.
//!
//! Migration 0014. Two tables, one durable job: make a SECOND ingest of a
//! conversation cost only the turns that are new — and make a rescan, when the
//! file rotated out from under the reader, cost nothing at all.
//!
//! # Two facts, not one
//!
//! The cursor alone survives only the happy path. A reader whose file rotated
//! or was truncated cannot resume: it restarts from zero and reports
//! [`ReadBatch::rescanned`], and what comes back is the WHOLE conversation.
//! The ledger is what makes that harmless — one row per record ever stored,
//! carrying the store's own natural id and the `turn_no` it went in under.
//!
//! The ledger maps rather than merely remembers. Dedupe needs only the key,
//! but `turn_no` is the navigation axis (req-0026) and half the primary key
//! [`crate::conversations::append_turns`] is idempotent on, so keeping the
//! number means a rescan RECOVERS a record's existing position instead of
//! minting a second one for the same content.
//!
//! # No trait here
//!
//! Ruled by the plan-005 PM on 2026-08-28 against this unit's own brief: this
//! crate has no trait convention to join, and a trait whose only second
//! implementation is its test fake does not clear workshop 001 rule 3. The
//! decisions worth testing in memory are pure and live in
//! [`fs3_core::conversation_normalize`]; what is left here is SQL, and SQL is
//! proven against Postgres.
//!
//! # Frames
//!
//! Timestamps stay server-side (the rule [`crate::messages`] sets out) and
//! guids cross as text and are cast at the query edge, because
//! [`ConversationId`] has already proven the shape. The cursor itself crosses
//! as TEXT cast to `jsonb`, exactly as `turns.items` does: one definition of
//! what a cursor is, and it is the Rust type.

use std::collections::BTreeSet;

use fs3_core::{ConversationId, Harness, SourceCursor};
use sqlx::Row;

use crate::{PgPool, StoreError};

/// What one poll needs to know, as one consistent snapshot.
///
/// Both answers come from ONE transaction because they are used together: a
/// `seen` set read before another poll's commit and a high-water mark read
/// after it would number a batch on top of turns it also decided to store.
///
/// A snapshot is PER POLL. Caching one across polls numbers the second batch
/// on top of the first — the optimisation a future reader adds in good faith,
/// named here so it is refused in review.
///
/// # What this fixes, and what it does NOT
///
/// Numbering from the conversation's stored turns fixes COLLISION: a tailed
/// turn lands ABOVE whatever the conversation already holds instead of
/// silently vanishing into an idempotent conflict.
///
/// It does NOT fix DEDUPE across ingest paths. Turns that arrived by
/// `fs3_cli::conversation` transcript import carry no ordinal — there is no
/// ledger row and nothing to match them on — so tailing the same session
/// afterwards APPENDS the same content beside them rather than recognising
/// it. That is deliberate v1 behaviour, not an oversight: the alternative is
/// matching turns by content hash across two paths that disagree about
/// payload shaping, which is a plan of its own. Import a transcript or tail
/// the session, not both.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LedgerView {
    /// Which of the asked-about ordinals are already stored. Scoped to the
    /// SESSION: an ordinal means nothing outside the session that minted it.
    pub seen: BTreeSet<String>,
    /// The number the next new turn takes. Scoped to the CONVERSATION, whose
    /// primary key it is — the stored turns' high-water mark plus one, and 1
    /// for a conversation that holds no turns at all.
    pub next_turn_no: u32,
}

/// What forgetting a session reclaimed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Forgotten {
    /// Whether a cursor was there to forget.
    pub existed: bool,
    /// Ledger rows that went with it.
    pub ledger_rows: i64,
}

/// Where the last read of this session stopped, if it has ever been read.
///
/// `None` means "start from the beginning", which is the same thing the
/// contract's `read_incremental(None)` means — so a first ingest and a
/// forgotten one take the identical path with no branch at the call site.
///
/// # Errors
/// [`StoreError::Query`] when the read fails; [`StoreError::Corrupt`] when the
/// stored JSON is not a cursor this build can express — which is what a
/// downgrade past a new [`SourceCursor`] variant would look like.
pub async fn load_cursor(
    pool: &PgPool,
    harness: Harness,
    session_id: &str,
) -> Result<Option<SourceCursor>, StoreError> {
    let stored: Option<serde_json::Value> = sqlx::query_scalar(
        "SELECT cursor FROM ingest_cursors WHERE harness = $1 AND session_id = $2",
    )
    .bind(harness.as_str())
    .bind(session_id)
    .fetch_optional(pool)
    .await?;

    stored
        .map(|value| {
            serde_json::from_value(value).map_err(|error| {
                StoreError::Corrupt(fs3_core::Error::InvalidConfig(format!(
                    "stored cursor for {harness}/{session_id} is not a source cursor: {error}"
                )))
            })
        })
        .transpose()
}

/// Which of `ordinals` are already stored, and what number the next turn takes.
///
/// Asks about the batch's ordinals rather than loading the whole ledger: a
/// long-running seat is thousands of rows and a poll only ever needs to know
/// about the handful it just read.
///
/// # Two different scopes, on purpose
///
/// `seen` is scoped to the SESSION, because an ordinal is the store's natural
/// id and means nothing outside the session that minted it — two sessions
/// sharing a conversation must not dedupe against each other.
///
/// `next_turn_no` is scoped to the CONVERSATION, and comes from the stored
/// turns rather than from this ledger. `turn_no` is the conversation's primary
/// key, so the conversation is the thing that owns the number; deriving it
/// from a per-session ledger was an INFERENCE about a one-session-per-
/// conversation mapping, and it fails in two ordinary ways. A conversation
/// previously filled by `fs3_cli::conversation` transcript import has turns
/// but no ledger, so an inferred mark would restart at 1 — and because
/// [`crate::conversations::append_turns`] is idempotent on
/// `(conversation_id, turn_no)`, every colliding turn would be SILENTLY
/// dropped while this module recorded it as stored. A second session on one
/// conversation fails the same way by a different door. Asking the turns
/// themselves cannot be forgotten and cannot drift. (Ruled by the plan-005 PM
/// on 2026-08-28.)
///
/// This fixes COLLISION, not DEDUPE — see the note on [`LedgerView`].
///
/// # Errors
/// [`StoreError::Query`] when the read fails; [`StoreError::Corrupt`] when a
/// stored `turn_no` is not a position in a sequence.
pub async fn ledger_view(
    pool: &PgPool,
    harness: Harness,
    session_id: &str,
    conversation: &ConversationId,
    ordinals: &[&str],
) -> Result<LedgerView, StoreError> {
    let mut tx = pool.begin().await?;

    let seen: BTreeSet<String> = sqlx::query_scalar(
        "SELECT ordinal FROM ingest_ledger
          WHERE harness = $1 AND session_id = $2 AND ordinal = ANY($3::text[])",
    )
    .bind(harness.as_str())
    .bind(session_id)
    .bind(ordinals)
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .collect();

    // The conversation's own high-water mark, from the rows that hold the
    // number. An index-only scan of the turns primary key.
    let highest: i32 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(turn_no), 0) FROM turns WHERE conversation_id = $1::uuid",
    )
    .bind(conversation.as_str())
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;

    let highest = u32::try_from(highest).map_err(|_| {
        StoreError::Corrupt(fs3_core::Error::InvalidConfig(format!(
            "turn_no {highest} in conversation {conversation} is not a position in a sequence"
        )))
    })?;

    Ok(LedgerView {
        seen,
        next_turn_no: highest + 1,
    })
}

/// Record what a poll stored and where to resume, in ONE transaction.
///
/// Atomic on purpose, and this is the failure it exists to refuse: a cursor
/// that advanced without its ledger rows leaves the next rescan unable to
/// recognise turns that ARE stored, so it appends the conversation a second
/// time under fresh numbers that the `(conversation_id, turn_no)` key cannot
/// catch. Ledger rows without a cursor are merely a re-read; the other way
/// round is a duplicated conversation.
///
/// Safe to call with an empty `ledger`: a poll that found nothing still moved
/// the reader's cursor over the bytes it inspected, and forgetting that is a
/// full re-read next time.
///
/// # Errors
/// [`StoreError::SessionRebound`] when this session is already tailing a
/// DIFFERENT conversation; nothing is written.
/// [`StoreError::Query`] when the transaction fails; nothing is written.
/// [`StoreError::Corrupt`] when the cursor cannot be serialised.
pub async fn commit_poll(
    pool: &PgPool,
    harness: Harness,
    session_id: &str,
    conversation: &ConversationId,
    cursor: &SourceCursor,
    ledger: &[(&str, u32)],
) -> Result<(), StoreError> {
    let encoded = serde_json::to_string(cursor).map_err(|error| {
        StoreError::Corrupt(fs3_core::Error::InvalidConfig(format!(
            "cursor for {harness}/{session_id} cannot be stored: {error}"
        )))
    })?;

    let mut tx = pool.begin().await?;

    // A session may not move conversations. The ledger is keyed
    // `(harness, session_id, ordinal)` and carries no conversation, so its
    // rows do NOT move with a rebind: afterwards the ledger insists every
    // record is stored while the new conversation holds nothing, and
    // `prepare_batch` dedupes the whole batch to zero. The conversation stays
    // permanently empty and every call reports success — the worst shape a
    // failure can take, and invisible from every angle. So this is an error,
    // not an update. (Ruled by the plan-005 PM on 2026-08-28; the real fix is
    // that resolution is a lookup rather than a mint, and that is the
    // composition root's.)
    //
    // Compared as `uuid` rather than as text so a difference in spelling is
    // not mistaken for a difference in identity, and `FOR UPDATE` so two
    // concurrent first polls cannot both see no row and insert under
    // different conversations.
    let rebound: Option<String> = sqlx::query_scalar(
        "SELECT conversation_id::text FROM ingest_cursors
          WHERE harness = $1 AND session_id = $2 AND conversation_id <> $3::uuid
          FOR UPDATE",
    )
    .bind(harness.as_str())
    .bind(session_id)
    .bind(conversation.as_str())
    .fetch_optional(&mut *tx)
    .await?;

    if let Some(stored) = rebound {
        return Err(StoreError::SessionRebound {
            harness: harness.as_str().to_string(),
            session_id: session_id.to_string(),
            stored,
            offered: conversation.as_str().to_string(),
        });
    }

    sqlx::query(
        "INSERT INTO ingest_cursors
           (harness, session_id, conversation_id, cursor, last_read_at)
         VALUES ($1, $2, $3::uuid, $4::jsonb, now())
         ON CONFLICT (harness, session_id) DO UPDATE SET
           conversation_id = EXCLUDED.conversation_id,
           cursor          = EXCLUDED.cursor,
           last_read_at    = now()",
    )
    .bind(harness.as_str())
    .bind(session_id)
    .bind(conversation.as_str())
    .bind(&encoded)
    .execute(&mut *tx)
    .await?;

    if !ledger.is_empty() {
        let ordinals: Vec<&str> = ledger.iter().map(|(ordinal, _)| *ordinal).collect();
        let turn_nos: Vec<i32> = ledger
            .iter()
            .map(|(_, turn_no)| i32::try_from(*turn_no).unwrap_or(i32::MAX))
            .collect();

        // ON CONFLICT DO NOTHING rather than an update: an ordinal's turn_no is
        // assigned once and never moves. A second attempt to write one is a
        // retry of a committed poll, and the number already there is the right
        // answer — overwriting it would renumber a stored turn.
        sqlx::query(
            "INSERT INTO ingest_ledger (harness, session_id, ordinal, turn_no)
             SELECT $1, $2, l.ordinal, l.turn_no
               FROM unnest($3::text[], $4::int[]) AS l(ordinal, turn_no)
             ON CONFLICT (harness, session_id, ordinal) DO NOTHING",
        )
        .bind(harness.as_str())
        .bind(session_id)
        .bind(&ordinals)
        .bind(&turn_nos)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    Ok(())
}

/// Which conversation this session is already being ingested into.
///
/// The reverse of [`sessions_for`], and the reason it exists is resolution:
/// `None` means NO session row yet, so this is a first ingest and the caller
/// mints exactly one conversation; `Some` means the mapping is already decided
/// and there is nothing left to choose. Minting where a row exists is the A1
/// failure — the ledger is keyed by session and would not move with the
/// rebind, so the newly minted conversation would dedupe every record it was
/// offered and stay permanently empty.
///
/// The asymmetry is deliberate and there is no mint helper here: this unit can
/// answer WHICH conversation a session belongs to, it cannot invent one.
/// Minting stays at the composition root, where the caller-supplied guid and
/// the CLI live.
///
/// Reads the same row [`commit_poll`] defends, so the lookup and the guard
/// cannot disagree.
///
/// # Errors
/// [`StoreError::Query`] when the read fails; [`StoreError::Corrupt`] when the
/// stored guid is not a conversation id.
pub async fn conversation_for(
    pool: &PgPool,
    harness: Harness,
    session_id: &str,
) -> Result<Option<ConversationId>, StoreError> {
    let stored: Option<String> = sqlx::query_scalar(
        "SELECT conversation_id::text FROM ingest_cursors
          WHERE harness = $1 AND session_id = $2",
    )
    .bind(harness.as_str())
    .bind(session_id)
    .fetch_optional(pool)
    .await?;

    stored.map(ConversationId::new).transpose().map_err(corrupt)
}

fn corrupt(error: fs3_core::Error) -> StoreError {
    StoreError::Corrupt(error)
}

/// Forget a session's cursor and everything it had stored.
///
/// The ledger goes with the cursor through the foreign key, so re-ingesting
/// afterwards is a clean first read rather than a rescan that dedupes against
/// rows whose turns may no longer exist.
///
/// # Errors
/// [`StoreError::Query`] when the transaction fails; nothing is deleted.
pub async fn forget_session(
    pool: &PgPool,
    harness: Harness,
    session_id: &str,
) -> Result<Forgotten, StoreError> {
    let mut tx = pool.begin().await?;

    // Counted before the delete: `ingest_ledger` is ON DELETE CASCADE, so
    // after it there is nothing left to count.
    let ledger_rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM ingest_ledger WHERE harness = $1 AND session_id = $2",
    )
    .bind(harness.as_str())
    .bind(session_id)
    .fetch_one(&mut *tx)
    .await?;

    let existed = sqlx::query("DELETE FROM ingest_cursors WHERE harness = $1 AND session_id = $2")
        .bind(harness.as_str())
        .bind(session_id)
        .execute(&mut *tx)
        .await?
        .rows_affected()
        > 0;

    tx.commit().await?;

    Ok(Forgotten {
        existed,
        ledger_rows,
    })
}

/// Every session still being tailed for a conversation.
///
/// One conversation can have several: a Claude session is a main file plus N
/// subagent sidecars, and each is cursored separately (recipe gotcha 6).
///
/// # Errors
/// [`StoreError::Query`] when the read fails; [`StoreError::Corrupt`] when a
/// stored harness is not one this build knows.
pub async fn sessions_for(
    pool: &PgPool,
    conversation: &ConversationId,
) -> Result<Vec<(Harness, String)>, StoreError> {
    let rows = sqlx::query(
        "SELECT harness, session_id FROM ingest_cursors
          WHERE conversation_id = $1::uuid
          ORDER BY harness, session_id",
    )
    .bind(conversation.as_str())
    .fetch_all(pool)
    .await?;

    rows.iter()
        .map(|row| {
            let stored: String = row.try_get("harness")?;
            let harness = stored.parse().map_err(StoreError::Corrupt)?;
            Ok((harness, row.try_get("session_id")?))
        })
        .collect()
}

/// Hold a conversation against concurrent polls, for the life of `held`.
///
/// The queue does NOT serialise ingest per conversation, which cross-model
/// review established rather than assumed: `SERIAL_KINDS` means claimed one at
/// a time, not RUN one at a time, so several ingest jobs can be in flight at
/// once — and two live queue keys can address ONE conversation, because the seat
/// route and the native route produce different keys for the same session.
///
/// That matters because [`ledger_view`] reads the conversation's own high-water
/// mark: two polls that read it before either commits will number against the
/// same value, and the second one's turns collide on `(conversation_id,
/// turn_no)` and are dropped.
///
/// # Why TRY rather than wait
///
/// `Ok(None)` means another poll of this conversation holds it. The caller
/// leaves the work to that poll rather than blocking, which is right twice
/// over: the other poll is reading the same bytes, so waiting buys nothing —
/// and a BLOCKING lock would let concurrent ingests each pin a connection while
/// queueing for one, which review round 2 showed can deadlock a fixed-size pool
/// (eight holders waiting on a ninth connection that cannot exist).
///
/// # Why the connection is detached
///
/// The lock is SESSION-scoped, so it lives with the connection rather than with
/// a transaction — a poll spans several transactions and a transaction-scoped
/// lock would let the next one in halfway through. A pooled connection returns
/// to the pool on drop with its session, and therefore its lock, intact: a
/// panic or a cancelled future would leak the hold and block every later poll
/// of that conversation. Detaching makes drop CLOSE the connection instead, so
/// Postgres releases the lock on every path — including the ones no `unlock`
/// call can reach.
///
/// # Errors
/// [`StoreError::Query`] when the connection or either lock statement fails.
pub async fn try_with_conversation_lock<T, F, Fut>(
    pool: &PgPool,
    conversation: &ConversationId,
    held: F,
) -> Result<Option<T>, StoreError>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = T>,
{
    let key = advisory_key(conversation);
    // Detached: dropping it closes the session, which is what makes the lock
    // release panic- and cancellation-safe. It also keeps a held lock from
    // occupying a pool slot for the length of a poll.
    let mut connection = pool.acquire().await?.detach();

    let taken: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1)")
        .bind(key)
        .fetch_one(&mut connection)
        .await?;
    if !taken {
        return Ok(None);
    }

    let outcome = held().await;

    // Best-effort: dropping `connection` below releases the lock regardless, so
    // an error here is not worth failing a completed poll over.
    let _ = sqlx::query("SELECT pg_advisory_unlock($1)")
        .bind(key)
        .execute(&mut connection)
        .await;
    Ok(Some(outcome))
}

/// A stable 64-bit key for `pg_advisory_lock`, derived from the conversation.
///
/// Computed here rather than with Postgres `hashtext`, so the key does not
/// depend on a hash function the database is free to change between versions —
/// a lock whose key moved under an upgrade would silently stop serialising.
fn advisory_key(conversation: &ConversationId) -> i64 {
    let digest = fs3_core::content_hash(conversation.as_str().as_bytes());
    let hex: String = digest
        .chars()
        .filter(char::is_ascii_hexdigit)
        .take(16)
        .collect();
    u64::from_str_radix(&hex, 16).map_or(0, |value| value as i64)
}
