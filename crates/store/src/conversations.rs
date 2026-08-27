//! Conversations: headers, turns, and the bridge into the content layer.
//!
//! Migration 0013. Two ref-layer tables and one join key — `turns.blob_sha` is
//! the content address of a turn's canonical stored form, and the same value is
//! the `blob_sha` of an `elements` row of kind `turn`. Everything expensive
//! hangs off that element exactly as it does for code: one summary, one pair of
//! vectors, one spend guard, one garbage collector (workshop 005, C1).
//!
//! # Why turns are written here and not through [`crate::elements`]
//!
//! [`crate::elements::upsert_element_tree`] writes a TREE: it walks parents to
//! children assigning ids as it descends, which is why it is a row at a time.
//! A batch of turns is flat — every turn element is rootless — so it goes in as
//! one multi-row statement instead of one round trip per turn. A transcript is
//! thousands of turns, and the difference is the whole import.
//!
//! # Frames and identity
//!
//! Timestamps stay server-side: nothing here reads a `TIMESTAMPTZ` into Rust
//! (the rule [`crate::messages`] sets out), so `at` and `started_at` cross the
//! wire as Postgres-formatted RFC 3339 text and two machines cannot disagree
//! about what a moment was. Guids cross as text and are cast at the query edge,
//! because [`ConversationId`] has already proven the shape.
//!
//! `blob_sha` is DERIVED on read rather than selected, the same bargain
//! [`crate::elements`] makes with `raw_hash`: deriving it is what makes "the
//! stored form changed" mean "the content changed", instead of letting a wrong
//! row pass itself off as right.

use std::str::FromStr;

use fs3_core::conversation::PARSER_VERSION;
use fs3_core::{Conversation, ConversationId, Element, ElementKind, Turn, TurnRole, TurnSource};
use sqlx::Row;
use sqlx::postgres::PgRow;

use crate::{PgPool, StoreError};

/// Postgres' spelling of RFC 3339 in UTC. One formatter for every timestamp
/// this module hands out, decided here rather than by each caller.
const AS_TEXT: &str = "'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"'";

/// How much of a turn's first line an outline row carries.
///
/// An outline is the cheap browse (`tree conv:<guid>`): it exists so a caller
/// can decide WHICH turns to pay for. A row that carries a whole paragraph has
/// already spent the tokens the outline was meant to save.
const OUTLINE_WIDTH: i32 = 200;

/// What one [`append_turns`] call actually did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Appended {
    /// The turns that were NOT already stored, as content-layer elements.
    ///
    /// Elements rather than ordinals because this is exactly what enrichment
    /// needs next — address, canonical text and its hash — and re-deriving them
    /// in the caller would be the same work twice. Empty on a re-post, which is
    /// what makes "enqueue only the delta" a fact rather than a policy.
    pub accepted: Vec<Element>,
    /// How many posted turns were already stored, unchanged.
    pub already_stored: usize,
}

/// What removing a conversation reclaimed directly.
///
/// Summaries and vectors are absent on purpose: they are keyed by `raw_hash`
/// and may still be shared, so reclaiming them is GC's decision at level two
/// and three, not this delete's (workshop 002, D8).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Removed {
    /// Whether there was a conversation to remove.
    pub existed: bool,
    /// Turn rows deleted.
    pub turns: i64,
    /// Turn element rows deleted.
    pub elements: i64,
}

/// A conversation as `conversation list` reports it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConversationSummary {
    /// The conversation's guid.
    pub guid: ConversationId,
    /// Anchor: the repository identity, when it has one.
    pub repo_identity: Option<String>,
    /// Anchor: the checkout path.
    pub worktree: Option<String>,
    /// Anchor: the commit the conversation started from.
    pub base_sha: Option<String>,
    /// Optional title.
    pub title: Option<String>,
    /// When it began, RFC 3339 in UTC.
    pub started_at: String,
    /// How many turns are stored.
    pub turns: i64,
}

/// Which conversations to list.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AnchorFilter<'a> {
    /// Only conversations anchored to this repository identity.
    pub repo: Option<&'a str>,
    /// Only conversations whose anchor worktree starts with this path.
    pub path_prefix: Option<&'a str>,
    /// Only the conversation with this guid — the "read one header" case,
    /// spelled as a filter so there is one listing query rather than two that
    /// can disagree about what a conversation row looks like.
    pub guid: Option<&'a str>,
}

/// One row of a conversation's turn outline.
///
/// Deliberately lean: no body, no items — just enough to choose a turn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TurnOutline {
    /// Position in the conversation.
    pub turn_no: u32,
    /// Who spoke.
    pub role: TurnRole,
    /// Where the turn came from.
    pub source: TurnSource,
    /// When, RFC 3339 in UTC.
    pub at: String,
    /// The first line of the prose, cut to a browsable width.
    pub first_line: String,
    /// How many typed sub-items the turn carries.
    pub items: i32,
}

/// Create or refresh a conversation header.
///
/// Append-friendly by construction (req-0027): a conversation is posted many
/// times as it grows, so this must be safe to call on every post.
///
/// Two rules make a re-post harmless. Anchor fields and the title are
/// COALESCED, so a later post that does not mention the title cannot erase one
/// an earlier import derived — a growing conversation only ever learns more
/// about itself. And `started_at` takes the EARLIEST of the two, because a
/// conversation cannot begin later than it already began; a client re-posting
/// with the timestamp of its latest batch would otherwise walk the start
/// forward until the anchor was a lie.
///
/// # Errors
/// [`StoreError::Query`] when the statement fails.
pub async fn upsert_conversation(
    pool: &PgPool,
    conversation: &Conversation,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO conversations
           (guid, repo_identity, worktree, base_sha, title, started_at)
         VALUES ($1::uuid, $2, $3, $4, $5, $6::timestamptz)
         ON CONFLICT (guid) DO UPDATE SET
           repo_identity = COALESCE(EXCLUDED.repo_identity, conversations.repo_identity),
           worktree      = COALESCE(EXCLUDED.worktree,      conversations.worktree),
           base_sha      = COALESCE(EXCLUDED.base_sha,      conversations.base_sha),
           title         = COALESCE(EXCLUDED.title,         conversations.title),
           started_at    = LEAST(EXCLUDED.started_at, conversations.started_at)",
    )
    .bind(conversation.guid.as_str())
    .bind(conversation.repo_identity.as_deref())
    .bind(conversation.worktree.as_deref())
    .bind(conversation.base_sha.as_deref())
    .bind(conversation.title.as_deref())
    .bind(&conversation.started_at)
    .execute(pool)
    .await?;

    Ok(())
}

/// Append turns to a conversation, storing only the ones not already there.
///
/// Idempotent on `(conversation_id, turn_no)`: re-posting an overlapping batch
/// stores nothing, returns no accepted elements, and — because the caller
/// enqueues enrichment from [`Appended::accepted`] — pays no provider a second
/// time. That is the iterative-append contract, enforced by the primary key
/// rather than by a check somebody has to remember to write.
///
/// A turn and its element are written in ONE transaction. A turn without its
/// element is a stored turn nothing can find, enrich or collect, and it would
/// be invisible: the next append skips the turn on conflict and would never
/// create the missing element.
///
/// `enrich` is the caller's injected size-gate verdict, not this module's
/// (decision D5, the same bargain [`crate::elements::upsert_element_tree`]
/// makes): the store records whether a turn earns its own summary, and the
/// threshold lives in config where it belongs.
///
/// # Errors
/// [`StoreError::Query`] when the transaction fails; nothing is written.
pub async fn append_turns(
    pool: &PgPool,
    conversation: &ConversationId,
    turns: &[Turn],
    enrich: impl Fn(&Element) -> bool,
) -> Result<Appended, StoreError> {
    if turns.is_empty() {
        return Ok(Appended::default());
    }

    // The elements are built once, up front: each carries the canonical text
    // and its hash, and both statements below need them.
    let elements: Vec<Element> = turns
        .iter()
        .map(|turn| turn.element(conversation))
        .collect();

    let ordinals: Vec<i32> = turns
        .iter()
        .map(|turn| i32::try_from(turn.turn_no).unwrap_or(i32::MAX))
        .collect();
    let roles: Vec<&str> = turns.iter().map(|turn| turn.role.as_str()).collect();
    let sources: Vec<&str> = turns.iter().map(|turn| turn.source.as_str()).collect();
    let head_shas: Vec<Option<&str>> = turns.iter().map(|turn| turn.head_sha.as_deref()).collect();
    let ats: Vec<&str> = turns.iter().map(|turn| turn.at.as_str()).collect();
    let bodies: Vec<&str> = turns.iter().map(|turn| turn.body.as_str()).collect();
    // Items cross as TEXT and are cast to `jsonb` in the statement: a text
    // array is a wire shape every driver agrees on, and the cast is the same
    // parse the column would do anyway.
    let items: Vec<String> = turns
        .iter()
        .map(|turn| {
            serde_json::to_string(&turn.items).map_err(|error| {
                StoreError::Corrupt(fs3_core::Error::InvalidConfig(format!(
                    "turn {} items cannot be stored as json: {error}",
                    turn.turn_no
                )))
            })
        })
        .collect::<Result<_, _>>()?;
    let blobs: Vec<&str> = elements.iter().map(|element| element.raw_hash()).collect();

    let mut tx = pool.begin().await?;

    let accepted_ordinals: Vec<i32> = sqlx::query_scalar(
        "INSERT INTO turns
           (conversation_id, turn_no, role, source, head_sha, at, body, items, blob_sha)
         SELECT $1::uuid, t.turn_no, t.role, t.source, t.head_sha,
                t.at::timestamptz, t.body, t.items::jsonb, t.blob_sha
           FROM unnest($2::int[], $3::text[], $4::text[], $5::text[],
                       $6::text[], $7::text[], $8::text[], $9::text[])
             AS t(turn_no, role, source, head_sha, at, body, items, blob_sha)
         ON CONFLICT (conversation_id, turn_no) DO NOTHING
         RETURNING turn_no",
    )
    .bind(conversation.as_str())
    .bind(&ordinals)
    .bind(&roles)
    .bind(&sources)
    .bind(&head_shas)
    .bind(&ats)
    .bind(&bodies)
    .bind(&items)
    .bind(&blobs)
    .fetch_all(&mut *tx)
    .await?;
    let accepted_ordinals: std::collections::HashSet<i32> = accepted_ordinals.into_iter().collect();

    let accepted: Vec<Element> = elements
        .into_iter()
        .zip(&ordinals)
        .filter(|(_, ordinal)| accepted_ordinals.contains(*ordinal))
        .map(|(element, _)| element)
        .collect();

    if !accepted.is_empty() {
        write_turn_elements(&mut tx, &accepted, &enrich).await?;
    }

    tx.commit().await?;

    Ok(Appended {
        already_stored: turns.len() - accepted.len(),
        accepted,
    })
}

/// Write the content-layer rows for a batch of accepted turns.
///
/// Flat by construction — a turn element has no parent and no children — so
/// one multi-row insert replaces the tree walk [`crate::elements`] needs.
async fn write_turn_elements(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    accepted: &[Element],
    enrich: &impl Fn(&Element) -> bool,
) -> Result<(), StoreError> {
    let addresses: Vec<&str> = accepted
        .iter()
        .map(|element| element.address.as_str())
        .collect();
    let names: Vec<&str> = accepted
        .iter()
        .map(|element| element.name.as_str())
        .collect();
    let subkinds: Vec<&str> = accepted
        .iter()
        .map(|element| element.subkind.as_str())
        .collect();
    let spans: Vec<i32> = accepted
        .iter()
        .map(|element| i32::try_from(element.span.start_line).unwrap_or(i32::MAX))
        .collect();
    let texts: Vec<&str> = accepted
        .iter()
        .map(|element| element.raw_text.as_str())
        .collect();
    let hashes: Vec<&str> = accepted.iter().map(Element::raw_hash).collect();
    let verdicts: Vec<bool> = accepted.iter().map(enrich).collect();

    sqlx::query(
        "INSERT INTO elements
           (blob_sha, parser_version, parent_id, kind, subkind, name, address,
            span_start, span_end, sibling_order, raw_text, raw_hash, enrich)
         SELECT e.raw_hash, $1, NULL, $2, e.subkind, e.name, e.address,
                e.span, e.span, 0, e.raw_text, e.raw_hash, e.enrich
           FROM unnest($3::text[], $4::text[], $5::text[], $6::int[],
                       $7::text[], $8::text[], $9::bool[])
             AS e(subkind, name, address, span, raw_text, raw_hash, enrich)
         ON CONFLICT (blob_sha, parser_version, address, span_start) DO UPDATE SET
           subkind  = EXCLUDED.subkind,
           name     = EXCLUDED.name,
           raw_text = EXCLUDED.raw_text,
           raw_hash = EXCLUDED.raw_hash,
           enrich   = EXCLUDED.enrich",
    )
    .bind(PARSER_VERSION)
    .bind(ElementKind::Turn.as_str())
    .bind(&subkinds)
    .bind(&names)
    .bind(&addresses)
    .bind(&spans)
    .bind(&texts)
    .bind(&hashes)
    .bind(&verdicts)
    .execute(&mut **tx)
    .await?;

    Ok(())
}

/// The contiguous run of turns around `turn_no`, in order.
///
/// The caller picks `before` and `after` and pays for exactly what it asked
/// for (workshop 003). Honest at the edges: a window that runs off either end
/// returns the turns that exist rather than padding, so the count a caller gets
/// back IS the evidence of where the conversation begins and ends.
///
/// # Errors
/// [`StoreError::Query`] when the read fails; [`StoreError::Corrupt`] when a
/// row carries a role, source or item shape the domain cannot express.
pub async fn window(
    pool: &PgPool,
    conversation: &ConversationId,
    turn_no: u32,
    before: u32,
    after: u32,
) -> Result<Vec<Turn>, StoreError> {
    // Widened before the arithmetic: `turn_no - before` underflows on a u32 for
    // every window that reaches past the first turn, which is the ordinary case.
    let centre = i64::from(turn_no);
    let first = centre - i64::from(before);
    let last = centre.saturating_add(i64::from(after));

    let rows = sqlx::query(&format!(
        "SELECT turn_no, role, source, head_sha, body, items,
                to_char(at AT TIME ZONE 'UTC', {AS_TEXT}) AS at
           FROM turns
          WHERE conversation_id = $1::uuid
            AND turn_no BETWEEN $2 AND $3
          ORDER BY turn_no"
    ))
    .bind(conversation.as_str())
    .bind(first)
    .bind(last)
    .fetch_all(pool)
    .await?;

    rows.iter().map(turn_from_row).collect()
}

/// Every turn of a conversation, lean: role, source, time and first line.
///
/// # Errors
/// [`StoreError::Query`] when the read fails; [`StoreError::Corrupt`] when a
/// row carries an unknown role or source.
pub async fn outline(
    pool: &PgPool,
    conversation: &ConversationId,
) -> Result<Vec<TurnOutline>, StoreError> {
    let rows = sqlx::query(&format!(
        "SELECT turn_no, role, source,
                to_char(at AT TIME ZONE 'UTC', {AS_TEXT}) AS at,
                left(split_part(body, E'\\n', 1), $2) AS first_line,
                jsonb_array_length(items) AS items
           FROM turns
          WHERE conversation_id = $1::uuid
          ORDER BY turn_no"
    ))
    .bind(conversation.as_str())
    .bind(OUTLINE_WIDTH)
    .fetch_all(pool)
    .await?;

    rows.iter()
        .map(|row| {
            Ok(TurnOutline {
                turn_no: ordinal_from_row(row)?,
                role: TurnRole::from_str(&row.try_get::<String, _>("role")?).map_err(corrupt)?,
                source: TurnSource::from_str(&row.try_get::<String, _>("source")?)
                    .map_err(corrupt)?,
                at: row.try_get("at")?,
                first_line: row.try_get("first_line")?,
                items: row.try_get("items")?,
            })
        })
        .collect()
}

/// Conversations matching an anchor filter, newest first, with turn counts.
///
/// The count is a correlated aggregate rather than a stored column, for the
/// reason [`crate::refs::list_worktrees`] gives: a cached counter is one more
/// thing that can be wrong.
///
/// `path_prefix` is a true prefix test (`strpos(... ) = 1`), not a `LIKE`
/// pattern. An anchor is a filesystem path and a path is allowed to contain
/// `_`, which `LIKE` would silently read as "any character" — a filter that
/// quietly matches more than it was asked to is worse than one that matches
/// nothing.
///
/// # Errors
/// [`StoreError::Query`] when the read fails; [`StoreError::Corrupt`] when a
/// stored guid is not a conversation id.
pub async fn list_conversations(
    pool: &PgPool,
    filter: AnchorFilter<'_>,
) -> Result<Vec<ConversationSummary>, StoreError> {
    let rows = sqlx::query(&format!(
        "SELECT c.guid::text AS guid, c.repo_identity, c.worktree, c.base_sha, c.title,
                to_char(c.started_at AT TIME ZONE 'UTC', {AS_TEXT}) AS started_at,
                (SELECT count(*) FROM turns t WHERE t.conversation_id = c.guid) AS turns
           FROM conversations c
          WHERE ($1::text IS NULL OR c.repo_identity = $1)
            AND ($2::text IS NULL OR strpos(coalesce(c.worktree, ''), $2) = 1)
            AND ($3::text IS NULL OR c.guid = $3::uuid)
          ORDER BY c.started_at DESC, c.guid"
    ))
    .bind(filter.repo)
    .bind(filter.path_prefix)
    .bind(filter.guid)
    .fetch_all(pool)
    .await?;

    rows.iter()
        .map(|row| {
            Ok(ConversationSummary {
                guid: ConversationId::new(row.try_get::<String, _>("guid")?).map_err(corrupt)?,
                repo_identity: row.try_get("repo_identity")?,
                worktree: row.try_get("worktree")?,
                base_sha: row.try_get("base_sha")?,
                title: row.try_get("title")?,
                started_at: row.try_get("started_at")?,
                turns: row.try_get("turns")?,
            })
        })
        .collect()
}

/// Remove a conversation, its turns and its turn elements.
///
/// Symmetric with `remove` for a root, and stopping in the same place: the
/// summaries and vectors the turns paid for are keyed by `raw_hash` and may be
/// shared with another conversation or with code, so they are left for GC to
/// re-derive as unreferenced. Deleting them here would be a cascade into
/// re-payable spend, which is exactly what decision D8 refuses.
///
/// Elements are matched by ADDRESS prefix, not by blob: `blob_sha` is shared
/// by construction — that sharing is the dedupe — so deleting by blob would
/// take another conversation's identical turn with it.
///
/// # Errors
/// [`StoreError::Query`] when the transaction fails; nothing is deleted.
pub async fn delete_conversation(
    pool: &PgPool,
    conversation: &ConversationId,
) -> Result<Removed, StoreError> {
    let mut tx = pool.begin().await?;

    // `FOR UPDATE` on the row itself, not on a count: two concurrent removals
    // of one conversation must not both report having removed it. Postgres
    // refuses row locking on an aggregate, and the aggregate would be the
    // wrong thing to lock anyway.
    let existed = sqlx::query("SELECT 1 FROM conversations WHERE guid = $1::uuid FOR UPDATE")
        .bind(conversation.as_str())
        .fetch_optional(&mut *tx)
        .await?
        .is_some();

    if !existed {
        return Ok(Removed::default());
    }

    // `conv:<guid>#t%`. The guid has been validated as hex-and-dashes, so it
    // carries no `LIKE` metacharacter of its own.
    let mut prefix = conversation.turn_address(0);
    prefix.truncate(prefix.len() - 1);
    prefix.push('%');

    let elements = sqlx::query("DELETE FROM elements WHERE kind = $1 AND address LIKE $2")
        .bind(ElementKind::Turn.as_str())
        .bind(&prefix)
        .execute(&mut *tx)
        .await?
        .rows_affected();

    // `turns` is ON DELETE CASCADE, so this is counted before it goes.
    let turns: i64 =
        sqlx::query_scalar("SELECT count(*) FROM turns WHERE conversation_id = $1::uuid")
            .bind(conversation.as_str())
            .fetch_one(&mut *tx)
            .await?;

    sqlx::query("DELETE FROM conversations WHERE guid = $1::uuid")
        .bind(conversation.as_str())
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    Ok(Removed {
        existed: true,
        turns,
        elements: i64::try_from(elements).unwrap_or(i64::MAX),
    })
}

/// Rebuild a turn from its row.
///
/// `blob_sha` is a stored column and is deliberately not read back: [`Turn`]
/// derives it from the canonical form, and deriving it is what makes "the
/// stored form changed" mean "the content changed".
fn turn_from_row(row: &PgRow) -> Result<Turn, StoreError> {
    let items: serde_json::Value = row.try_get("items")?;

    Ok(Turn {
        turn_no: ordinal_from_row(row)?,
        role: TurnRole::from_str(&row.try_get::<String, _>("role")?).map_err(corrupt)?,
        source: TurnSource::from_str(&row.try_get::<String, _>("source")?).map_err(corrupt)?,
        head_sha: row.try_get("head_sha")?,
        at: row.try_get("at")?,
        body: row.try_get("body")?,
        items: serde_json::from_value(items).map_err(|error| {
            StoreError::Corrupt(fs3_core::Error::InvalidConfig(format!(
                "turn items are not a stored item list: {error}"
            )))
        })?,
    })
}

/// `turn_no` is `INT` in the column and `u32` in the domain, and the check
/// constraint keeps it positive — so a negative here is a corrupt row, not a
/// conversion to shrug at.
fn ordinal_from_row(row: &PgRow) -> Result<u32, StoreError> {
    let stored: i32 = row.try_get("turn_no")?;
    u32::try_from(stored).map_err(|_| {
        StoreError::Corrupt(fs3_core::Error::InvalidConfig(format!(
            "turn ordinal {stored} is not a position in a sequence"
        )))
    })
}

fn corrupt(error: fs3_core::Error) -> StoreError {
    StoreError::Corrupt(error)
}
