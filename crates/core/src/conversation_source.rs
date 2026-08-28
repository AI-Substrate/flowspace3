//! Reading agent conversations out of the native session stores (req-0027).
//!
//! Four stores hold the same kind of thing in four shapes — Claude Code's
//! session jsonl, omp's session jsonl, the pij seat ledger, and git-ai's
//! machine-wide sqlite metrics — and every one of them is APPEND-ONLY. That is
//! the whole design: a conversation is read by remembering where you stopped,
//! so a second ingest of a session that grew costs only the turns that are new.
//!
//! # The seam
//!
//! [`ConversationSource`] is the third port in fs3, ruled by the plan-005
//! impl-guide on 2026-08-28: workshop 001 rule 3 asks for a second real
//! implementation before a trait earns its existence, and this one ships with
//! FOUR on day one. Its shape is FROZEN — the four readers were written in
//! parallel against it, so widening it is not a refactor, it is a schedule.
//! Coders fill this contract; they never widen it. A fifth store, or a method
//! this does not have, is a stop-and-ask.
//!
//! # Why this is not async
//!
//! Every implementation is blocking IO — `read_at`, `stat`, a directory glob,
//! a sqlite query — and none of it is a network call worth an executor. The
//! composition root hands these to `spawn_blocking`, exactly as it does the
//! local ONNX embedder, so the trait stays object-safe without `async_trait`
//! and the readers stay testable from an ordinary `#[test]`.
//!
//! # Why records are already semantic
//!
//! [`RawRecord`] carries [`TurnRole`], [`TurnSource`] and [`TurnItem`] — the
//! same vocabulary the intake endpoint speaks — rather than a store's own JSON.
//! Dialects live in the reader and nowhere else, which is the rule
//! `fs3_cli::conversation` already set for imported transcripts. What is left
//! for the normaliser is genuinely pure: assign the ordinal, apply the payload
//! policy, drop what v1 does not store.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::conversation::{TurnItem, TurnRole, TurnSource};
use crate::error::Result;

/// Which native store a conversation is read from.
///
/// The harness picks the STORE; the folder picks the workspace-slugged
/// directory inside it. `MetricsDb` is the odd one — it is machine-wide, holds
/// every repository at once, and is the only place a copilot session exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Harness {
    /// Claude Code native: `~/.claude/projects/<slug>/<uuid>.jsonl` and its
    /// sidecar directory of subagent conversations.
    Claude,
    /// omp / pi native: `~/.omp/agent/sessions/<slug>/<ts>_<uuid>.jsonl`.
    Omp,
    /// The pij seat ledger: `~/.pij/<seat>/events.ndjson`, keyed by SEAT rather
    /// than by session uuid, and the only store that holds delivery receipts.
    PijLedger,
    /// git-ai's machine-wide sqlite metrics, `event_kind = 5`.
    MetricsDb,
}

impl Harness {
    /// The wire spelling, which is also what `--harness` accepts.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Omp => "omp",
            Self::PijLedger => "pij",
            Self::MetricsDb => "metrics-db",
        }
    }
}

impl std::str::FromStr for Harness {
    type Err = crate::error::Error;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        match value {
            "claude" => Ok(Self::Claude),
            "omp" | "pi" => Ok(Self::Omp),
            "pij" | "pij-ledger" => Ok(Self::PijLedger),
            "metrics-db" | "metrics_db" | "git-ai" => Ok(Self::MetricsDb),
            other => Err(crate::error::Error::InvalidConfig(format!(
                "unknown harness {other:?}: expected claude, omp, pij or metrics-db"
            ))),
        }
    }
}

impl std::fmt::Display for Harness {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What an ingest was asked for.
///
/// Two routes to the same conversation, because the two ways an operator knows
/// a session are a fleet SEAT and a harness's own uuid. [`IngestInput::Pij`]
/// resolves to the second through the `pij sessions` join; both must land the
/// same turns, which is what plan-005 ac-0002 proves by content hash.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "by")]
pub enum IngestInput {
    /// Addressed by pij seat id — the harness and native session id are looked
    /// up, and for [`Harness::PijLedger`] the seat IS the address.
    Pij {
        /// The seat, e.g. `pij-linguistic-narwhal`.
        id: String,
        /// The workspace the conversation happened in.
        folder: PathBuf,
    },
    /// Addressed by the harness's own session id, no join required.
    Native {
        /// The store's session identifier: a uuid for claude/omp/copilot.
        session_id: String,
        /// Which store holds it.
        harness: Harness,
        /// The workspace the conversation happened in.
        folder: PathBuf,
    },
}

impl IngestInput {
    /// The workspace both routes carry.
    #[must_use]
    pub fn folder(&self) -> &std::path::Path {
        match self {
            Self::Pij { folder, .. } | Self::Native { folder, .. } => folder,
        }
    }
}

/// Whether a resolved file is the conversation itself or a child of it.
///
/// One Claude SESSION is a main jsonl plus N subagent sidecars, and a sidecar
/// that is ingested into the parent's sequence makes both unreadable. They are
/// separate conversations, linked (recipe gotcha 6).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionKind {
    /// The conversation named by the input.
    Main,
    /// A child conversation — a claude subagent sidecar — linked to its parent.
    Subagent,
}

/// One readable unit of a resolved conversation.
///
/// A file for the three jsonl stores; for [`Harness::MetricsDb`] the `path` is
/// the database and `session_id` is the `external_session_id` being read, so a
/// single database yields one `SessionFile` per session rather than one per
/// file. That is what keeps the cursor per-conversation everywhere.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionFile {
    /// Where the bytes are.
    pub path: PathBuf,
    /// The store's identifier for this conversation.
    pub session_id: String,
    /// The parent conversation, for a [`SessionKind::Subagent`].
    pub parent_session_id: Option<String>,
    /// Main conversation or child.
    pub kind: SessionKind,
    /// Which store produced it, so the orchestrator can route without guessing.
    pub harness: Harness,
}

/// Where reading stopped, in the only terms each store can resume from.
///
/// Timestamps are NOT a cursor anywhere: metrics-db stamps at second grain and
/// omp emits equal ISO stamps inside a burst, so both collide precisely when a
/// conversation is busiest (recipe §3). Every variant here is monotonic and
/// exact.
///
/// Serialisable because [`SourceCursor`] is what the store persists between
/// polls — a cursor that only lives in memory makes the second ingest a full
/// re-read.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum SourceCursor {
    /// Append-only jsonl: resume at a byte offset, with the file's identity
    /// alongside it so a rotation is detectable rather than assumed.
    ByteOffset {
        /// `st_dev` of the file the offset belongs to.
        device: u64,
        /// `st_ino` of the file the offset belongs to.
        inode: u64,
        /// Bytes consumed, always landing after a complete line.
        offset: u64,
    },
    /// The pij ledger's monotonic event sequence — the cleanest cursor of the
    /// four, because it survives the file being rewritten entirely.
    Seq {
        /// The highest `seq` already read.
        seq: u64,
    },
    /// sqlite's `rowid`, which is monotonic per insert and unique where
    /// `event_ts` is neither.
    RowId {
        /// The highest `rowid` already read.
        rowid: i64,
    },
}

/// One record as a store wrote it, translated into fs3's turn vocabulary.
///
/// This is deliberately NOT a `serde_json::Value`: the dialect stops at the
/// reader. A record here has already survived its store's quirks — claude's
/// one-line-per-content-block merge, omp's `xd://` tool remap, copilot's
/// `type`-not-`name` event naming — so the normaliser downstream is pure
/// arithmetic over a settled shape.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawRecord {
    /// The store's natural identifier for this record: claude `uuid`, omp
    /// record `id`, ledger `seq`, metrics-db `rowid`.
    ///
    /// This is the DEDUPE key. After a rotation forces a rescan from zero, it
    /// is the only thing that distinguishes a record already stored from one
    /// that is new.
    pub ordinal: String,
    /// The record this one answered, where the store records a chain.
    pub parent_ordinal: Option<String>,
    /// When it happened, RFC 3339 in UTC.
    pub at: String,
    /// Who spoke.
    pub role: TurnRole,
    /// Where it came from — the axis that tells a human turn from an injected
    /// peer packet from a compaction the harness wrote (recipe gotcha 5).
    pub source: TurnSource,
    /// The prose, verbatim.
    pub body: String,
    /// Tool calls and results, in the order they happened.
    pub items: Vec<TurnItem>,
    /// Repo HEAD at time-of-record, when the store knows it.
    pub head_sha: Option<String>,
}

/// One incremental read: what was new, where to resume, and whether the file
/// moved under us.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadBatch {
    /// The records after the supplied cursor, in store order.
    pub records: Vec<RawRecord>,
    /// Where the next read resumes.
    pub cursor: SourceCursor,
    /// Whether the reader had to start over.
    ///
    /// True when the file was rotated or truncated — a different inode, or a
    /// size below the offset we held. The records are then the WHOLE file, not
    /// a delta, and the caller must dedupe on [`RawRecord::ordinal`] before
    /// appending. Silence here would look identical to a burst of new turns
    /// and would duplicate an entire conversation.
    pub rescanned: bool,
}

impl ReadBatch {
    /// An empty batch that leaves the cursor where it was — what a poll of an
    /// unchanged file returns.
    #[must_use]
    pub const fn unchanged(cursor: SourceCursor) -> Self {
        Self {
            records: Vec::new(),
            cursor,
            rescanned: false,
        }
    }

    /// Whether this poll found anything.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}

/// A native store fs3 can read conversations out of.
///
/// FROZEN 2026-08-28 (plan 005 phase 1). Four implementations were written in
/// parallel against this shape; a change here is not a refactor, it is a
/// re-plan. Fill the contract, never widen it — a method this trait does not
/// have is a stop-and-ask to the plan's PM.
///
/// Every implementation must satisfy the shared contract suite in
/// `fs3_testkit::conversation_source`, which is the mechanical definition of
/// "done" for a reader: resolve finds the files, a read from `None` yields
/// everything with a cursor, a re-read from that cursor yields nothing,
/// appended bytes yield only the delta, and a half-written record yields no
/// torn output.
pub trait ConversationSource: Send + Sync {
    /// Which store this reads.
    fn harness(&self) -> Harness;

    /// Find every file that makes up the addressed conversation.
    ///
    /// Called on EVERY poll, not once: claude subagent sidecars appear
    /// mid-session, and a sidecar discovered on the fourth poll is a child
    /// conversation that starts at offset zero (recipe §3). A reader that
    /// resolves once loses every subagent spawned after ingestion began.
    ///
    /// # Errors
    /// [`crate::error::Error::Provider`] when the store cannot be read or the
    /// input names a session this store does not hold.
    fn resolve(&self, input: &IngestInput) -> Result<Vec<SessionFile>>;

    /// Read what is new since `cursor`, or everything when it is `None`.
    ///
    /// Contract:
    /// * Records are returned in store order, and only COMPLETE ones — a
    ///   writer mid-line at read time must yield nothing rather than half a
    ///   record, and the returned cursor must not advance past it.
    /// * The returned cursor is always resumable, including for an empty batch.
    /// * When the underlying file rotated or was truncated, the reader restarts
    ///   from zero and says so via [`ReadBatch::rescanned`].
    ///
    /// # Errors
    /// [`crate::error::Error::Provider`] when the store cannot be read, or when
    /// `cursor` is a variant this store does not use.
    fn read_incremental(
        &self,
        file: &SessionFile,
        cursor: Option<&SourceCursor>,
    ) -> Result<ReadBatch>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn harness_round_trips_through_its_wire_spelling() {
        for harness in [
            Harness::Claude,
            Harness::Omp,
            Harness::PijLedger,
            Harness::MetricsDb,
        ] {
            let parsed: Harness = harness.as_str().parse().expect("wire spelling parses");
            assert_eq!(parsed, harness, "{harness} must survive a round trip");
        }
    }

    #[test]
    fn the_harness_aliases_operators_actually_type_are_accepted() {
        assert_eq!("pi".parse::<Harness>().expect("pi is omp"), Harness::Omp);
        assert_eq!(
            "git-ai"
                .parse::<Harness>()
                .expect("git-ai is the metrics db"),
            Harness::MetricsDb
        );
        assert!(
            "cursor".parse::<Harness>().is_err(),
            "an unknown harness must be refused, not guessed at"
        );
    }

    #[test]
    fn a_cursor_survives_serialisation_because_the_store_persists_it() {
        let cursor = SourceCursor::ByteOffset {
            device: 16_777_232,
            inode: 42,
            offset: 4096,
        };
        let json = serde_json::to_string(&cursor).expect("cursor serialises");
        let back: SourceCursor = serde_json::from_str(&json).expect("cursor deserialises");
        assert_eq!(back, cursor);
        assert!(
            json.contains("byte_offset"),
            "the tag is part of the persisted shape: {json}"
        );
    }

    #[test]
    fn an_unchanged_poll_keeps_its_cursor_and_claims_no_rescan() {
        let cursor = SourceCursor::Seq { seq: 17 };
        let batch = ReadBatch::unchanged(cursor.clone());
        assert!(batch.is_empty());
        assert_eq!(batch.cursor, cursor);
        assert!(
            !batch.rescanned,
            "an unchanged poll that claimed a rescan would force a full dedupe pass"
        );
    }

    #[test]
    fn both_input_routes_carry_the_folder_that_picks_the_slug() {
        let pij = IngestInput::Pij {
            id: "pij-linguistic-narwhal".into(),
            folder: PathBuf::from("/w/fs3"),
        };
        let native = IngestInput::Native {
            session_id: "01a0-".into(),
            harness: Harness::Omp,
            folder: PathBuf::from("/w/fs3"),
        };
        assert_eq!(pij.folder(), native.folder());
    }
}
