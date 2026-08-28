//! The git-ai metrics store: machine-wide sqlite, `event_kind = 5` (plan 005, u1d).
//!
//! This store is the odd one of the four. The other three are per-session files
//! under a user's home; this is ONE database holding every repository on the
//! machine at once, written continuously by a tool that is not fs3. Three
//! consequences shape everything below.
//!
//! # 1. Repo scoping is not optional, and the type says so
//!
//! An unscoped read of this store leaks other projects' conversations into this
//! index. That is not a rare edge — the committed fixture carries rows from
//! `github.com/AI-Substrate/pij` sitting between rows from this repo, because
//! that is what the live store looks like. So [`MetricsDbSource::new`] REQUIRES
//! a [`RepoScope`]: there is no `Default`, no unscoped constructor and no
//! `Option`. The mistake is not caught at runtime, it is unwritable.
//!
//! The scope key is the repository remote URL, which the store records at
//! `$.a."1"` on every row of both dialects. Scoping on the field the store
//! indexes by is the point: the plan packet originally proposed
//! `event_json LIKE '%flowspace3%'`, which is a substring search over
//! conversation PROSE — it matches a foreign row that merely mentions this repo
//! and misses one that never names it. The `LIKE` count survives as a test
//! tripwire over frozen bytes, where it is fine, and nowhere else.
//!
//! # 2. The cursor is the `id` column, and it survives a VACUUM
//!
//! `event_ts` is second-grain and NOT unique — 17 timestamps in the fixture
//! carry more than one row — so it collides precisely when a conversation is
//! busiest. The `id` column is monotonic per insert and unique.
//!
//! A sqlite `rowid` is normally NOT stable: `VACUUM` renumbers rows. That would
//! be fatal here, because a renumbered store breaks every ordinal already
//! persisted, all at once, in someone else's database that we do not control.
//! It does not apply, and this is the evidence — the store's own DDL:
//!
//! ```sql
//! CREATE TABLE metrics ( id INTEGER PRIMARY KEY AUTOINCREMENT, event_json TEXT NOT NULL, ... )
//! ```
//!
//! `INTEGER PRIMARY KEY` makes `id` an ALIAS for the rowid, and sqlite only
//! renumbers tables that lack one, so `VACUUM` cannot move these values.
//! `AUTOINCREMENT` additionally guarantees an id is never reused after a
//! deletion. The queries below therefore name `id` explicitly rather than the
//! bare `rowid` keyword: they are the same value, and naming the aliased column
//! is what makes that stability visible to whoever reads the query next.
//!
//! This database SELF-PRUNES (`schema_metadata` carries a
//! `metrics_last_prune_ts` watermark). A prune can drop rows out from under a
//! held cursor, after which a naive reader's cursor exceeds every row it can
//! see and it returns empty forever — indistinguishable from a quiet
//! conversation, which is the exact failure this plan exists to prevent. So a
//! held cursor above `max(rowid)` for the SCOPED session is reported as
//! [`ReadBatch::rescanned`] and re-read from zero; the durable ordinal ledger
//! deduplicates it back to nothing. A session with no rows in scope is not a
//! prune — it is a session with no rows, and it must not look like one.
//!
//! # 3. We are a guest in another tool's database
//!
//! The live store was 4.2 GB with a 47 MB uncheckpointed WAL at fixture-harvest
//! time. This reader opens it `file:...?mode=ro`, holds one prepared statement
//! per call and no transaction across calls: it must never write, never
//! checkpoint and never hold a long read against a database somebody else is
//! actively appending to.
//!
//! # The ordinal derivation is FROZEN — changing it doubles every conversation
//!
//! **The ordinal is the decimal string form of the row's `id`. For rows merged
//! by `message.id`, it is the FIRST `id` of the group.**
//!
//! This is not an implementation detail and it is not yours to tidy. The
//! ordinal is the key the durable cursor ledger deduplicates on, and that key
//! is written to Postgres, where it outlives every process. Change how it is
//! derived — a different field, a different rendering, first-of-group becoming
//! last-of-group, the integer becoming something prettier — and every record
//! already stored looks brand new on the next poll: the conversation SILENTLY
//! DOUBLES, and there is no clean recovery, because re-reading from zero
//! duplicates it again. First-of-group specifically is what makes the key
//! stable across a re-read that regroups the same blocks; last-of-group would
//! change between polls and the dedupe would miss.
//!
//! ## The GROUPING RULE is frozen too, and it is the sharper edge
//!
//! This reader's ordinal is group-derived, so it depends on a datum AND on the
//! rule that decides group membership. Two of the four readers are like this;
//! the other two key on a record and carry strictly less risk. The frozen rule
//! here, in full:
//!
//! * THE ROW-SELECTION PREDICATE: only rows matching `event_kind = 5` AND the
//!   repo scope exist to be grouped at all. The scope is applied PER ROW, so it
//!   is part of the grouping rule rather than a filter that happens before one.
//! * Of the rows that survive, only `tool = 'claude'` records of type `user` or
//!   `assistant` are emitted. Every other record type is dropped.
//! * Of those, rows carrying `message.id` merge into ONE record per distinct
//!   `message.id`. Rows without one — every `user` row — never merge.
//! * The record's ordinal is the smallest `id` in its group.
//!
//! All four are the frozen rule. Widen the emit allowlist, let a new type join
//! a merge, start including a row that is skipped today, OR CHANGE
//! `event_kind` OR THE SCOPE EXPRESSION, and the FIRST element of an existing
//! group can change even though the datum did not. Every stored record then
//! looks new and the conversation doubles — the same silent failure as changing
//! the derivation, reached by touching something that does not look like the
//! derivation at all.
//!
//! Measured, so the risk is sized rather than asserted: in the committed
//! fixture every one of the six sessions carries exactly ONE distinct
//! `$.a."1"`, with no NULL and no empty string in 100 rows. That is a sample,
//! not a guarantee — git-ai stamps the repo PER EVENT, so nothing structural
//! stops a long-lived session spanning a `git remote set-url` — and the fixture
//! cannot speak to `event_kind` at all, because the harvest selected
//! `event_kind = 5`. Treat the freeze as load-bearing.
//!
//! If you have a reason to change either, that is a conversation with the
//! plan's owner, not an edit.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use fs3_core::{
    ConversationSource, Error, Harness, IngestInput, RawRecord, ReadBatch, Result, SessionFile,
    SessionKind, SourceCursor, ToolInput, TurnItem, TurnRole, TurnSource,
};
use rusqlite::{Connection, OpenFlags};
use serde_json::Value;

/// The repository a read is confined to.
///
/// A newtype rather than a `String` parameter so that "which repo" cannot be
/// confused with "which session" at a call site, and so the unscoped read has
/// no spelling. Construct it from the repository's remote URL exactly as the
/// store records it at `$.a."1"`, e.g.
/// `https://github.com/AI-Substrate/flowspace3`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RepoScope(String);

impl RepoScope {
    /// Confine reads to the repository with this remote URL.
    ///
    /// Compared by exact equality against the store's own field. The value must
    /// be the remote as the writing tool recorded it — not a path, not a slug,
    /// and not a normalised form of either.
    #[must_use]
    pub fn remote_url(url: impl Into<String>) -> Self {
        Self(url.into())
    }

    /// The remote URL this scope matches.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Reads conversations out of git-ai's machine-wide sqlite metrics store.
///
/// One database, many repositories, two dialects. See the module docs for why
/// the scope is mandatory and why the cursor is `rowid`.
#[derive(Clone, Debug)]
pub struct MetricsDbSource {
    database: PathBuf,
    scope: RepoScope,
}

impl MetricsDbSource {
    /// Read `database`, confined to `scope`.
    ///
    /// There is deliberately no unscoped constructor: this store is
    /// machine-wide, so an unscoped read is a data leak rather than a
    /// convenience, and a type that cannot express the mistake outlives a test
    /// that merely catches it.
    #[must_use]
    pub fn new(database: impl Into<PathBuf>, scope: RepoScope) -> Self {
        Self {
            database: database.into(),
            scope,
        }
    }

    /// The database this reads.
    #[must_use]
    pub fn database(&self) -> &Path {
        &self.database
    }

    /// The repository reads are confined to.
    #[must_use]
    pub fn scope(&self) -> &RepoScope {
        &self.scope
    }

    /// Open read-only.
    ///
    /// Read-only is enforced twice over — the URI's `mode=ro` and the open
    /// flag — because the live store belongs to another tool and a writable
    /// handle to it is a hazard, not a capability.
    fn open(&self) -> Result<Connection> {
        let uri = read_only_uri(&self.database);
        Connection::open_with_flags(
            &uri,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
        )
        .map_err(|error| {
            Error::Provider(format!(
                "metrics-db: cannot open {} read-only: {error}",
                self.database.display()
            ))
        })
    }
}

/// `file:` URI for a read-only open, with the three characters sqlite would
/// otherwise read as URI syntax escaped.
///
/// A database path containing `?` is not hypothetical on a developer's machine,
/// and the failure it causes — sqlite silently parsing the tail as query
/// parameters — is deeply unobvious.
fn read_only_uri(path: &Path) -> String {
    let mut uri = String::from("file:");
    for character in path.to_string_lossy().chars() {
        match character {
            '?' => uri.push_str("%3f"),
            '#' => uri.push_str("%23"),
            '%' => uri.push_str("%25"),
            other => uri.push(other),
        }
    }
    uri.push_str("?mode=ro");
    uri
}

/// Which dialect a row is written in, decided by the store's own `tool` column.
///
/// Derived from the data, never from a hand-kept list of session ids: a list
/// goes stale the first time a new session appears and fails silently, by
/// reading a copilot conversation as a claude one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Dialect {
    /// git-ai's mirror of a Claude Code session: the native record sits at
    /// `v."0"` with its type at `v."0".type`.
    ClaudeMirror,
    /// The copilot event stream: an event name at `v."0".type` and a payload
    /// under `v."0".data`.
    Copilot,
}

impl Dialect {
    /// `None` for a tool this reader has no dialect for, which is a DROP.
    fn from_tool(tool: &str) -> Option<Self> {
        match tool {
            "claude" => Some(Self::ClaudeMirror),
            "github-copilot-cli" => Some(Self::Copilot),
            _ => None,
        }
    }
}

/// One row, decoded far enough to decide what it is.
struct Row {
    rowid: i64,
    dialect: Dialect,
    event: Value,
    event_ts: Option<i64>,
}

impl Row {
    /// The store's own record-type name, which BOTH dialects spell `type`.
    ///
    /// The plan packet said copilot carries this at `v."0".name`. It does not —
    /// no row in the store has that path, copilot's `v."0"` keys are exactly
    /// `{data, id, parentId, timestamp, type}`, and the frozen contract's own
    /// rustdoc names "copilot's `type`-not-`name` event naming". Confirmed a
    /// packet typo by the plan's PM, 2026-08-28.
    fn record_type(&self) -> Option<&str> {
        self.event.get("type")?.as_str()
    }

    /// RFC 3339 UTC, which both dialects record at `v."0".timestamp`.
    ///
    /// Falls back to the `event_ts` column for the live store. Every row this
    /// reader EMITS carries the ISO field in the committed fixture — the 14
    /// that lack it are all bookkeeping types that never reach here — so the
    /// fallback is robustness against a store that grows a new shape, not a
    /// path the fixture exercises.
    /// `None` when the row carries NEITHER, and the caller must then DROP the
    /// row. This returned an empty string until 2026-08-28, which is worse than
    /// it sounds: `append_turns` casts `at::timestamptz` and an empty string
    /// ERRORS, so the turn-plus-element transaction writes nothing — and
    /// because `commit_poll` runs after the append, the cursor never advances
    /// either. The next poll re-reads the same bytes, hits the same row, and
    /// fails identically: a PERMANENT STALL on that one session, unrecoverable
    /// without a code change.
    ///
    /// Dropping is the discipline this module already applies to every other
    /// row it cannot interpret — an unknown tool, an unknown event type, an
    /// unparseable `event_json` — and in each of those the cursor still
    /// advances, so a reader cannot be stopped by one bad row.
    ///
    /// The general rule this instance earned (u2, 2026-08-28): a fallback may
    /// be a VALUE or a HOLE, and only the first is safe. Claude's reader
    /// defaults an absent timestamp to a 1970 sentinel, which stores fine and
    /// reads as obviously wrong to a human. An empty string stores nothing and
    /// stalls the session. Same construct, opposite outcomes.
    fn at(&self) -> Option<String> {
        self.event
            .get("timestamp")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| self.event_ts.map(rfc3339_from_unix_seconds))
    }
}

/// Seconds since the unix epoch as RFC 3339 UTC.
///
/// Hand-rolled because this crate has no date dependency and earning one for a
/// fallback path would be a poor trade. The civil-from-days algorithm is
/// Howard Hinnant's, which is exact for the whole range that matters here.
fn rfc3339_from_unix_seconds(seconds: i64) -> String {
    let days = seconds.div_euclid(86_400);
    let time_of_day = seconds.rem_euclid(86_400);

    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;

    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);

    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        time_of_day / 3600,
        (time_of_day % 3600) / 60,
        time_of_day % 60,
    )
}

/// The rowid cursor, or a refusal.
///
/// A cursor from another store must be REFUSED rather than read as zero: read
/// as zero it would silently re-ingest an entire conversation, and the caller
/// would see a burst of duplicates with no error to explain them.
fn rowid_from(cursor: Option<&SourceCursor>) -> Result<i64> {
    match cursor {
        None => Ok(0),
        Some(SourceCursor::RowId { rowid }) => Ok(*rowid),
        Some(other) => Err(Error::Provider(format!(
            "metrics-db resumes from a rowid cursor; refusing {other:?} from another store \
             rather than reading it as zero and re-ingesting the conversation"
        ))),
    }
}

impl ConversationSource for MetricsDbSource {
    fn harness(&self) -> Harness {
        Harness::MetricsDb
    }

    fn resolve(&self, input: &IngestInput) -> Result<Vec<SessionFile>> {
        let session_id = match input {
            IngestInput::Native { session_id, .. } => session_id.clone(),
            IngestInput::Pij { id, .. } => {
                return Err(Error::Provider(format!(
                    "metrics-db is addressed by native session id; the pij seat {id:?} is \
                     resolved to one by the orchestrator's join, not by this reader"
                )));
            }
        };

        let connection = self.open()?;

        // Scoped on purpose: an out-of-scope session must be invisible here, not
        // merely unread later.
        let present: i64 = connection
            .query_row(
                "select count(*) from metrics \
                 where event_kind = 5 \
                   and external_session_id = ?1 \
                   and json_extract(event_json, '$.a.\"1\"') = ?2",
                (&session_id, self.scope.as_str()),
                |row| row.get(0),
            )
            .map_err(|error| {
                Error::Provider(format!("metrics-db: counting {session_id}: {error}"))
            })?;

        if present == 0 {
            return Err(Error::Provider(format!(
                "metrics-db holds no rows for session {session_id} in {}",
                self.scope.as_str()
            )));
        }

        let mut files = vec![SessionFile {
            path: self.database.clone(),
            session_id: session_id.clone(),
            parent_session_id: None,
            kind: SessionKind::Main,
            harness: Harness::MetricsDb,
        }];

        // Re-queried on EVERY resolve, never cached: a subagent that starts on
        // the fourth poll is a child conversation the reader must find then, and
        // resolving once loses it. `external_parent_session_id` is a real column,
        // so this costs no JSON parsing.
        let mut statement = connection
            .prepare(
                "select external_session_id, min(id) as first_row from metrics \
                 where event_kind = 5 \
                   and external_parent_session_id = ?1 \
                   and json_extract(event_json, '$.a.\"1\"') = ?2 \
                 group by external_session_id \
                 order by first_row",
            )
            .map_err(|error| {
                Error::Provider(format!("metrics-db: preparing child query: {error}"))
            })?;

        let children = statement
            .query_map((&session_id, self.scope.as_str()), |row| {
                row.get::<_, String>(0)
            })
            .map_err(|error| Error::Provider(format!("metrics-db: listing children: {error}")))?;

        for child in children {
            let child = child
                .map_err(|error| Error::Provider(format!("metrics-db: reading child: {error}")))?;
            files.push(SessionFile {
                path: self.database.clone(),
                session_id: child,
                parent_session_id: Some(session_id.clone()),
                kind: SessionKind::Subagent,
                harness: Harness::MetricsDb,
            });
        }

        Ok(files)
    }

    fn read_incremental(
        &self,
        file: &SessionFile,
        cursor: Option<&SourceCursor>,
    ) -> Result<ReadBatch> {
        let held = rowid_from(cursor)?;
        let connection = self.open()?;
        let session_id = &file.session_id;

        // Scoped to this session, not the whole store: a prune is a claim about
        // the rows this cursor can see, and `max(rowid)` over everything would
        // let a busy unrelated repository mask one.
        let highest: Option<i64> = connection
            .query_row(
                "select max(id) from metrics \
                 where event_kind = 5 \
                   and external_session_id = ?1 \
                   and json_extract(event_json, '$.a.\"1\"') = ?2",
                (session_id, self.scope.as_str()),
                |row| row.get(0),
            )
            .map_err(|error| {
                Error::Provider(format!("metrics-db: max rowid for {session_id}: {error}"))
            })?;

        let Some(highest) = highest else {
            // No rows in scope at all. That is an empty session, NOT a pruned
            // one, and calling it a rescan would make every empty poll re-read.
            return Ok(ReadBatch::unchanged(SourceCursor::RowId { rowid: held }));
        };

        let rescanned = held > highest;
        let from = if rescanned { 0 } else { held };

        let mut statement = connection
            .prepare(
                "select id, tool, event_json, event_ts from metrics \
                 where event_kind = 5 \
                   and external_session_id = ?1 \
                   and json_extract(event_json, '$.a.\"1\"') = ?2 \
                   and id > ?3 \
                 order by id",
            )
            .map_err(|error| Error::Provider(format!("metrics-db: preparing read: {error}")))?;

        let scanned = statement
            .query_map((session_id, self.scope.as_str(), from), |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<i64>>(3)?,
                ))
            })
            .map_err(|error| Error::Provider(format!("metrics-db: reading rows: {error}")))?;

        let mut rows = Vec::new();
        let mut furthest = from;
        for scanned_row in scanned {
            let (rowid, tool, event_json, event_ts) = scanned_row
                .map_err(|error| Error::Provider(format!("metrics-db: decoding row: {error}")))?;
            furthest = furthest.max(rowid);

            // A row this reader cannot interpret advances the cursor and emits
            // nothing. An ingest must not fail because the store grew a shape
            // no one told us about — and it must not stall on it either.
            let Some(dialect) = tool.as_deref().and_then(Dialect::from_tool) else {
                continue;
            };
            let Ok(envelope) = serde_json::from_str::<Value>(&event_json) else {
                continue;
            };
            let Some(event) = envelope.pointer("/v/0").cloned() else {
                continue;
            };

            rows.push(Row {
                rowid,
                dialect,
                event,
                event_ts,
            });
        }

        let records = assemble(&rows);
        let cursor = SourceCursor::RowId {
            rowid: if rows.is_empty() && !rescanned {
                held.max(furthest)
            } else {
                furthest
            },
        };

        Ok(ReadBatch {
            records,
            cursor,
            rescanned,
        })
    }
}

/// Turn decoded rows into records, in store order.
fn assemble(rows: &[Row]) -> Vec<RawRecord> {
    let mut records: Vec<RawRecord> = Vec::new();
    // message.id -> index into `records`, so a later block folds into the turn
    // its siblings already opened.
    let mut open_groups: BTreeMap<String, usize> = BTreeMap::new();
    // toolCallId -> index, for copilot's split call/result events.
    // toolCallId -> (record index, the tool the START named). The NAME travels
    // with the index because a turn can hold several calls, and labelling a
    // result from the record's FIRST call mislabels every one after it.
    let mut open_calls: BTreeMap<String, (usize, String)> = BTreeMap::new();

    for row in rows {
        match row.dialect {
            Dialect::ClaudeMirror => claude_row(row, &mut records, &mut open_groups),
            Dialect::Copilot => copilot_row(row, &mut records, &mut open_calls),
        }
    }

    records
}

/// One claude-mirror row.
///
/// Records sharing a `message.id` are ONE turn: the store writes one row per
/// content block, so an assistant message that thought, spoke and called two
/// tools arrives as four rows. Emitting them separately would report a single
/// answer as four turns and make the conversation unreadable.
fn claude_row(row: &Row, records: &mut Vec<RawRecord>, open_groups: &mut BTreeMap<String, usize>) {
    let Some(record_type) = row.record_type() else {
        return;
    };

    // An EMIT allowlist, not a skip list: the store's bookkeeping vocabulary
    // (attachment, queue-operation, mode, permission-mode, last-prompt,
    // custom-title, agent-name, atis-latch, pr-link, system, file-history-delta,
    // file-history-snapshot) grows without telling us, and a skip list silently
    // promotes every new bookkeeping type to a conversation turn.
    let role = match record_type {
        "user" => TurnRole::Human,
        "assistant" => TurnRole::Agent,
        _ => return,
    };

    let message = row.event.get("message");
    let group = message
        .and_then(|message| message.get("id"))
        .and_then(Value::as_str);

    // A user turn has no message.id and must never be merged; only the
    // assistant's block groups fold.
    if let Some(index) = group.and_then(|group| open_groups.get(group).copied()) {
        let (body, items) = claude_content(message, row);
        append_into(&mut records[index], &body, items);
        return;
    }

    // A row this reader cannot date is a row it cannot interpret, and it is
    // dropped like any other — see `Row::at`. Dropping happens HERE rather than
    // at the top of the function so a continuation block still folds into the
    // turn its siblings opened: a fold contributes body, never a timestamp.
    let Some(at) = row.at() else {
        return;
    };

    let (body, items) = claude_content(message, row);
    let source = claude_source(row, role, &body);

    records.push(RawRecord {
        ordinal: row.rowid.to_string(),
        parent_ordinal: row
            .event
            .get("parentUuid")
            .and_then(Value::as_str)
            .map(str::to_owned),
        at,
        role,
        source,
        body,
        items,
        // This store records a repo remote and a branch, never a HEAD sha.
        // Claiming one would be an invention; the orchestrator supplies it.
        head_sha: None,
    });

    if let Some(group) = group {
        open_groups.insert(group.to_owned(), records.len() - 1);
    }
}

/// Where a claude-mirror turn came FROM, which is not who wrote it.
///
/// Three sources, and getting this wrong reports an orchestrated agent fleet as
/// half-human (workshop 005, C8).
fn claude_source(row: &Row, role: TurnRole, body: &str) -> TurnSource {
    if role == TurnRole::Agent {
        return TurnSource::System;
    }
    // A compaction summary is written into the transcript BY the harness and
    // wears a user turn's clothes. It is never dropped — it is the only record
    // of what the discarded context said (recipe gotcha 5).
    if row
        .event
        .get("isCompactSummary")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return TurnSource::System;
    }
    // A packet injected by another agent in the fleet. The store gives no flag
    // for it, but the fleet's own wire format is unambiguous in the body.
    if body.starts_with("[pij from ") {
        return TurnSource::Peer;
    }
    TurnSource::Human
}

/// Prose and tool items out of a claude `message.content`.
///
/// `content` is either a bare string or an array of typed blocks, and both
/// shapes appear on user rows in the committed fixture.
fn claude_content(message: Option<&Value>, row: &Row) -> (String, Vec<TurnItem>) {
    let content = message
        .and_then(|message| message.get("content"))
        .or_else(|| row.event.get("content"));

    let mut body = String::new();
    let mut items = Vec::new();

    match content {
        Some(Value::String(text)) => body.push_str(text),
        Some(Value::Array(blocks)) => {
            for block in blocks {
                claude_block(block, &mut body, &mut items);
            }
        }
        _ => {}
    }

    (body, items)
}

/// One content block.
fn claude_block(block: &Value, body: &mut String, items: &mut Vec<TurnItem>) {
    // A bare tool_result object — the shape a user row carries — has no `type`.
    let kind = block
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_else(|| {
            if block.get("tool_use_id").is_some() {
                "tool_result"
            } else {
                ""
            }
        });

    match kind {
        // `text` only. A `thinking` block is NOT prose the agent said, and the
        // reference oracle does not render it either — so excluding it makes
        // agreement with the oracle definitional rather than lucky. The fixture
        // has six assistant groups whose only block is `thinking` and one whose
        // blocks are `text` + `tool_use`; folding thinking into `body` would
        // give the second a body the oracle never produced, and the committed
        // expectation compares that body by sha256.
        "text" => {
            if let Some(text) = block.get("text").and_then(Value::as_str) {
                push_prose(body, text);
            }
        }
        "tool_use" => items.push(TurnItem::ToolCall {
            tool: block
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_owned(),
            // Verbatim always. The write-family elision and the output head cap
            // are the normaliser's payload policy, applied once for every store
            // rather than four times slightly differently.
            input: ToolInput::Verbatim {
                text: render(block.get("input")),
            },
        }),
        "tool_result" => {
            let text = render(block.get("content"));
            let total = text.len() as u64;
            items.push(TurnItem::ToolResult {
                tool: block
                    .get("tool_use_id")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_owned(),
                head: text,
                total_bytes: total,
                truncated: false,
            });
        }
        _ => {}
    }
}

/// One copilot event-stream row.
///
/// PM-DERIVED, NOT ORACLE-BACKED. The pinned reference oracle produced ZERO
/// turns for this dialect (`oracle_turns: 0`), so nothing independent pins this
/// mapping: the only external check it gets is the structural claim that its
/// ordinals are an in-order, repeat-free subsequence of the ids the store
/// holds, which catches an invented or reordered record but cannot catch a
/// wrong allowlist. Ruled by the plan's PM 2026-08-28 and labelled here for the
/// same reason the claude fixtures are labelled.
fn copilot_row(
    row: &Row,
    records: &mut Vec<RawRecord>,
    open_calls: &mut BTreeMap<String, (usize, String)>,
) {
    let Some(record_type) = row.record_type() else {
        return;
    };
    let data = row.event.get("data");

    match record_type {
        "user.message" => {
            let body = data
                .and_then(|data| data.get("content"))
                .map(render_text)
                .unwrap_or_default();
            let source = if body.starts_with("[pij from ") {
                TurnSource::Peer
            } else {
                TurnSource::Human
            };
            records.extend(copilot_record(
                row,
                TurnRole::Human,
                source,
                body,
                Vec::new(),
            ));
        }
        "assistant.message" => {
            let body = data
                .and_then(|data| data.get("content"))
                .map(render_text)
                .unwrap_or_default();

            // Tool requests ride along on the message that made them, which is
            // where a reader of the transcript expects to find them.
            let mut items = Vec::new();
            let mut requested: Vec<(String, String)> = Vec::new();
            if let Some(requests) = data
                .and_then(|data| data.get("toolRequests"))
                .and_then(Value::as_array)
            {
                for request in requests {
                    let named = request
                        .get("toolName")
                        .or_else(|| request.get("name"))
                        .and_then(Value::as_str)
                        .unwrap_or("unknown")
                        .to_owned();
                    if let Some(id) = request
                        .get("id")
                        .or_else(|| request.get("toolCallId"))
                        .and_then(Value::as_str)
                    {
                        requested.push((id.to_owned(), named.clone()));
                    }
                    items.push(TurnItem::ToolCall {
                        tool: named,
                        input: ToolInput::Verbatim {
                            text: render(request.get("arguments").or_else(|| request.get("input"))),
                        },
                    });
                }
            }

            // The id association is registered AFTER the record is emitted, and
            // only if it was. `copilot_record` returns `None` for a row this
            // reader cannot date, so registering against `records.len()` first
            // — which round 4 of review caught — leaves an index one past the
            // end that a later completion dereferences and PANICS on.
            let before = records.len();
            records.extend(copilot_record(
                row,
                TurnRole::Agent,
                TurnSource::System,
                body,
                items,
            ));
            if records.len() > before {
                for (id, named) in requested {
                    open_calls.insert(id, (before, named));
                }
            }
        }
        "tool.execution_start" => {
            // NOT a turn, and NOT an item — the assistant.message that requested
            // this tool already converted its `toolRequests` entry into a
            // `TurnItem::ToolCall`. Round 1 caught this emitting a fifth record;
            // round 2 caught the first fix pushing a SECOND ToolCall for the
            // same call. All this event contributes is WHERE the result belongs
            // and WHICH tool it is for.
            let Some(call) = data
                .and_then(|data| data.get("toolCallId"))
                .and_then(Value::as_str)
            else {
                return;
            };
            let tool = data
                .and_then(|data| data.get("toolName"))
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_owned();

            // EXACT association first: the assistant request registered this id
            // against its own record. Only when the store omitted the id — which
            // it does in the committed fixture — fall back to the most recent
            // record already holding a ToolCall for this tool. Round 3 of review
            // named the difference: a name-anchored guess mis-attaches on a turn
            // that called two tools.
            // EXACT association first. The fallback is deliberately narrow: it
            // accepts ONLY the most recent record, and only if that record
            // already holds a ToolCall for this tool.
            //
            // Scanning further back would mis-attach in a case this reader
            // creates for itself: an assistant row that cannot be dated is
            // DROPPED, so if the turn that actually requested this tool was
            // dropped, an unrestricted walk-back lands on an OLDER turn that
            // happened to call the same tool. Requiring the last record means
            // the result is dropped instead, which is the rule this branch
            // already applies to a result whose call it never saw.
            let anchor = open_calls.get(call).map(|(index, _)| *index).or_else(|| {
                let last = records.len().checked_sub(1)?;
                records[last]
                    .items
                    .iter()
                    .any(|item| match item {
                        TurnItem::ToolCall { tool: named, .. } => *named == tool,
                        TurnItem::ToolResult { .. } => false,
                    })
                    .then_some(last)
            });
            let Some(index) = anchor else {
                // The assistant record that requested it is older than the
                // cursor. Dropping is right for the same reason a result whose
                // call we never saw is dropped.
                return;
            };
            open_calls.insert(call.to_owned(), (index, tool));
        }
        "tool.execution_complete" => {
            let call = data
                .and_then(|data| data.get("toolCallId"))
                .and_then(Value::as_str);
            let Some((index, tool)) = call.and_then(|call| open_calls.remove(call)) else {
                // A result whose call we never saw — the call is older than the
                // cursor. Dropping it is right: attaching it to the wrong turn
                // would be worse than not having it.
                return;
            };
            let text = render(data.and_then(|data| data.get("result")));
            let total = text.len() as u64;
            records[index].items.push(TurnItem::ToolResult {
                tool,
                head: text,
                total_bytes: total,
                truncated: false,
            });
        }
        // Everything else is bookkeeping: turn_start/turn_end, the eight
        // model.* telemetry events, session.* and hook.*. An event type this
        // reader has never heard of lands here too, and is DROPPED rather than
        // erroring — the twentieth type ships whenever GitHub feels like it, and
        // an ingest must not fail because a store grew a row.
        _ => {}
    }
}

/// A copilot record with the fields both of its emitting paths share.
fn copilot_record(
    row: &Row,
    role: TurnRole,
    source: TurnSource,
    body: String,
    items: Vec<TurnItem>,
) -> Option<RawRecord> {
    Some(RawRecord {
        ordinal: row.rowid.to_string(),
        parent_ordinal: row
            .event
            .get("parentId")
            .and_then(Value::as_str)
            .map(str::to_owned),
        at: row.at()?,
        role,
        source,
        body,
        items,
        head_sha: None,
    })
}

/// Fold a later block into the turn its siblings already opened.
fn append_into(record: &mut RawRecord, body: &str, items: Vec<TurnItem>) {
    push_prose(&mut record.body, body);
    record.items.extend(items);
}

/// Join prose with a blank line, without leading or doubled separators.
fn push_prose(body: &mut String, text: &str) {
    if text.is_empty() {
        return;
    }
    if !body.is_empty() {
        body.push_str("\n\n");
    }
    body.push_str(text);
}

/// A JSON value as text: strings bare, everything else as compact JSON.
///
/// Bare strings matter — a tool result that is already text must not arrive
/// downstream wrapped in quotes and escapes it never had.
fn render(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(text)) => text.clone(),
        Some(other) => other.to_string(),
    }
}

/// Prose out of a copilot `content`, which is a string or an array of parts.
fn render_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Array(parts) => {
            let mut body = String::new();
            for part in parts {
                let text = part
                    .get("text")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .unwrap_or_else(|| match part {
                        Value::String(text) => text.clone(),
                        _ => String::new(),
                    });
                push_prose(&mut body, &text);
            }
            body
        }
        other => render(Some(other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_foreign_cursor_is_refused_rather_than_read_as_zero() {
        let foreign = SourceCursor::ByteOffset {
            device: 1,
            inode: 2,
            offset: 3,
        };
        assert!(rowid_from(Some(&foreign)).is_err());
        assert!(rowid_from(Some(&SourceCursor::Seq { seq: 7 })).is_err());
        assert_eq!(rowid_from(None).unwrap(), 0);
        assert_eq!(
            rowid_from(Some(&SourceCursor::RowId { rowid: 42 })).unwrap(),
            42
        );
    }

    #[test]
    fn dialect_comes_from_the_tool_column_and_an_unknown_tool_is_a_drop() {
        assert_eq!(Dialect::from_tool("claude"), Some(Dialect::ClaudeMirror));
        assert_eq!(
            Dialect::from_tool("github-copilot-cli"),
            Some(Dialect::Copilot)
        );
        assert_eq!(Dialect::from_tool("some-tool-shipped-next-year"), None);
    }

    #[test]
    fn a_row_with_no_timestamp_at_all_is_dropped_rather_than_dated_with_a_hole() {
        // The poison pill this replaced: `at` returned an EMPTY STRING, and
        // `append_turns` casts `at::timestamptz`, where an empty string ERRORS.
        // The turn-plus-element transaction then writes nothing AND
        // `commit_poll` never runs, so the cursor does not advance and every
        // later poll re-reads the same row and fails identically — a permanent
        // stall on that one session.
        let dated = Row {
            rowid: 1,
            event_ts: None,
            event: serde_json::json!({"type": "user", "timestamp": "2026-08-28T01:00:00Z"}),
            dialect: Dialect::ClaudeMirror,
        };
        let from_column = Row {
            rowid: 2,
            event_ts: Some(1_787_817_816),
            event: serde_json::json!({"type": "user"}),
            dialect: Dialect::ClaudeMirror,
        };
        let undatable = Row {
            rowid: 3,
            event_ts: None,
            event: serde_json::json!({"type": "user"}),
            dialect: Dialect::ClaudeMirror,
        };

        assert_eq!(dated.at().as_deref(), Some("2026-08-28T01:00:00Z"));
        assert_eq!(from_column.at().as_deref(), Some("2026-08-27T08:03:36Z"));
        assert_eq!(
            undatable.at(),
            None,
            "neither field: the row cannot be dated, and a hole is not a date"
        );

        let mut records = Vec::new();
        let mut groups = BTreeMap::new();
        for row in [&dated, &from_column, &undatable] {
            claude_row(row, &mut records, &mut groups);
        }
        let ordinals: Vec<&str> = records.iter().map(|r| r.ordinal.as_str()).collect();
        assert_eq!(
            ordinals,
            ["1", "2"],
            "the undatable row is dropped, and the rows around it still emit"
        );
    }

    #[test]
    fn epoch_seconds_render_as_rfc3339_utc() {
        // The fixture's own first row: event_ts 1787817816.
        assert_eq!(
            rfc3339_from_unix_seconds(1_787_817_816),
            "2026-08-27T08:03:36Z"
        );
        assert_eq!(rfc3339_from_unix_seconds(0), "1970-01-01T00:00:00Z");
        // A leap day, because the civil-from-days arithmetic is where this
        // would go wrong and be believed anyway.
        assert_eq!(
            rfc3339_from_unix_seconds(1_709_164_800),
            "2024-02-29T00:00:00Z"
        );
    }

    #[test]
    fn a_database_path_containing_uri_syntax_is_escaped() {
        let uri = read_only_uri(Path::new("/tmp/odd?name#1/metrics.sqlite3"));
        assert_eq!(uri, "file:/tmp/odd%3fname%231/metrics.sqlite3?mode=ro");
    }

    #[test]
    fn prose_joins_without_leading_or_doubled_separators() {
        let mut body = String::new();
        push_prose(&mut body, "");
        assert_eq!(body, "");
        push_prose(&mut body, "first");
        push_prose(&mut body, "");
        push_prose(&mut body, "second");
        assert_eq!(body, "first\n\nsecond");
    }
}
