//! The `--watch` event stream: what the daemon says while it works.
//!
//! # Why this exists
//!
//! DL-045: every interface fs3 has grown so far had to MOCK activity, because
//! the daemon tells nobody what it is doing. The human-render prototype
//! inferred progress from poll deltas; the TUI prototype faked a feed; and
//! `flowspace3 add` sits silent for as long as a walk takes because there is no
//! channel on which to say "1,200 files so far". Any future web UI hits the
//! same wall. So the daemon gets one stream, and every interface reads it.
//!
//! # The wire
//!
//! NDJSON: one JSON object per line, UTF-8, `\n`-terminated, never pretty.
//! Lines are independent — a consumer that drops one keeps working, which is
//! the property that lets a TUI reconnect without a session.
//!
//! The FIRST line of every stream is a [`Hello`], so a consumer knows what it
//! is attached to before any event arrives and can refuse a version it cannot
//! read. Every line after it is an [`Event`].
//!
//! ```text
//! {"stream":"fs3.events","v":1,"daemon":"0.4.0","heartbeat_ms":15000}
//! {"v":1,"at":"2026-08-28T03:11:04.219Z","kind":"scan_progress","root":"git:…","files_seen":1200,…}
//! {"v":1,"at":"2026-08-28T03:11:05.001Z","kind":"job_done","job":"scan_file","subject":"src/lib.rs","ms":12,"left":455}
//! {"v":1,"at":"2026-08-28T03:11:20.000Z","kind":"heartbeat","seq":1}
//! ```
//!
//! # Forward compatibility is a requirement, not a nicety
//!
//! A daemon and a TUI can be different vintages, and the TUI is the one that
//! must not fall over. [`EventKind`] therefore carries an
//! [`Unknown`](EventKind::Unknown) variant: a kind this build has never heard
//! of parses, keeps its timestamp, and is simply not displayed. New kinds are
//! ADDITIVE and never a version bump; [`STREAM_VERSION`] moves only if the line
//! framing itself breaks.
//!
//! # What is NOT here
//!
//! No log lines. The stream reports work the daemon COMPLETED or is measurably
//! part-way through — things with a subject and a count — not the daemon's
//! narration of itself. `tracing` already owns that, it goes to the log file,
//! and mixing the two would make the stream unreadable at exactly the moment
//! someone needs it.

use serde::{Deserialize, Serialize};

/// The stream's framing version. Bumps only when a LINE's shape breaks — never
/// when a new [`EventKind`] is added.
pub const STREAM_VERSION: u32 = 1;

/// The stream's name, so a consumer that connected to the wrong port knows.
pub const STREAM_NAME: &str = "fs3.events";

/// How often the daemon emits a [`heartbeat`](EventKind::Heartbeat) when
/// nothing else is happening.
///
/// A number chosen once, here, rather than defaulted differently by each
/// consumer: a default is a number nobody chose. Fifteen seconds is short
/// enough that a dead stream is noticed inside one screen refresh and long
/// enough to be invisible in a log.
pub const HEARTBEAT_MS: u64 = 15_000;

/// The first line of every stream.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hello {
    /// Always [`STREAM_NAME`].
    pub stream: String,
    /// Always [`STREAM_VERSION`].
    pub v: u32,
    /// The daemon's version, so a consumer can explain a kind it does not know.
    pub daemon: String,
    /// The heartbeat cadence this stream will actually use.
    pub heartbeat_ms: u64,
}

impl Hello {
    /// The greeting for a daemon of `version`.
    #[must_use]
    pub fn new(version: impl Into<String>) -> Self {
        Hello {
            stream: STREAM_NAME.to_string(),
            v: STREAM_VERSION,
            daemon: version.into(),
            heartbeat_ms: HEARTBEAT_MS,
        }
    }
}

/// One thing that happened.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    /// Always [`STREAM_VERSION`].
    pub v: u32,
    /// When it happened, RFC 3339 in UTC, milliseconds included.
    ///
    /// Set by the daemon, never by the consumer — two clocks disagreeing would
    /// reorder a feed that is only useful in order.
    pub at: String,
    /// What happened.
    #[serde(flatten)]
    pub kind: EventKind,
}

impl Event {
    /// An event of `kind`, stamped `at`.
    #[must_use]
    pub fn new(at: impl Into<String>, kind: EventKind) -> Self {
        Event {
            v: STREAM_VERSION,
            at: at.into(),
            kind,
        }
    }
}

/// The event vocabulary.
///
/// Internally tagged on `kind`, so a line is flat and a consumer reads
/// `kind` without unwrapping anything — the same shape choice the envelope
/// made with `ok`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EventKind {
    /// A queued job settled successfully.
    ///
    /// Emitted where the runner already records completion, so the stream
    /// mirrors the daemon's existing lifecycle instead of inventing one.
    JobDone {
        /// `scan_file`, `summarize` or `embed`.
        job: String,
        /// What the job was about — a path, an address, a batch label.
        subject: String,
        /// How long it took.
        ms: u64,
        /// Jobs still queued after this one, so a consumer can draw a meter
        /// without polling `status`.
        left: i64,
    },
    /// A queued job did not settle.
    ///
    /// `terminal` is the whole point of carrying failures at all: a retry is
    /// noise, and a job that has stopped retrying is news.
    JobFailed {
        /// `scan_file`, `summarize` or `embed`.
        job: String,
        /// What the job was about.
        subject: String,
        /// The failure, in the daemon's words.
        error: String,
        /// How many times it has been tried.
        attempts: i64,
        /// Whether the daemon has given up on it.
        terminal: bool,
    },
    /// The queue's shape changed.
    ///
    /// A snapshot rather than a delta: a consumer that attached late, or
    /// dropped a line, is immediately correct again.
    Queue {
        /// One row per (kind, state), exactly as `status` reports them.
        rows: Vec<QueueDepth>,
    },
    /// A walk is under way — the answer to `add` sitting silent.
    ///
    /// Emitted DURING the walk `add`/`scan` performs before it answers, which
    /// is the only honest source for a progress indicator: the CLI cannot know
    /// what the daemon is looking at, and a spinner that knows nothing is a
    /// spinner that lies (Jordan, 2026-08-28).
    ScanProgress {
        /// The repository identity being walked.
        root: String,
        /// Its worktree path.
        root_path: String,
        /// Files seen so far.
        files_seen: u64,
        /// Scan jobs queued so far.
        enqueued: u64,
        /// The path being looked at when this line was written, repo-relative.
        ///
        /// Optional because a walk reports its tail-end totals without one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        current: Option<String>,
    },
    /// A root was registered, re-scanned or removed.
    RootChanged {
        /// `added`, `rescanned` or `removed`.
        change: String,
        /// The repository identity.
        root: String,
        /// Its worktree path.
        root_path: String,
        /// The root's file count after the change.
        files: i64,
    },
    /// Nothing is happening, and the stream is still alive.
    Heartbeat {
        /// Increments per stream, so a consumer can tell a stall from a gap.
        seq: u64,
    },
    /// A kind this build does not know.
    ///
    /// The forward-compatibility guarantee, in one variant: an older consumer
    /// against a newer daemon keeps its stream and skips what it cannot draw.
    #[serde(other)]
    Unknown,
}

/// One queue row, in the same vocabulary `status` uses.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueueDepth {
    /// `scan_file`, `summarize`, `embed`.
    pub kind: String,
    /// `pending`, `running`, `done`, `failed`.
    pub state: String,
    /// How many are in that state.
    pub count: i64,
}

#[cfg(test)]
mod tests {
    use super::{Event, EventKind, Hello, QueueDepth, STREAM_NAME, STREAM_VERSION};

    /// The line a consumer sees first, and what it promises.
    #[test]
    fn the_hello_line_names_the_stream_and_its_version() {
        let line = serde_json::to_string(&Hello::new("0.4.0")).expect("a hello serialises");
        assert_eq!(
            line,
            r#"{"stream":"fs3.events","v":1,"daemon":"0.4.0","heartbeat_ms":15000}"#
        );
    }

    /// Flat lines: `kind` sits beside `v` and `at`, not inside a wrapper.
    #[test]
    fn an_event_line_is_flat() {
        let event = Event::new(
            "2026-08-28T03:11:05.001Z",
            EventKind::JobDone {
                job: "scan_file".to_string(),
                subject: "src/lib.rs".to_string(),
                ms: 12,
                left: 455,
            },
        );

        assert_eq!(
            serde_json::to_string(&event).expect("an event serialises"),
            r#"{"v":1,"at":"2026-08-28T03:11:05.001Z","kind":"job_done","job":"scan_file","subject":"src/lib.rs","ms":12,"left":455}"#
        );
    }

    /// Every kind survives the wire unchanged — the contract u-w produces and
    /// u-t consumes, asserted in one place so neither has to trust prose.
    #[test]
    fn every_kind_round_trips() {
        let kinds = [
            EventKind::JobDone {
                job: "embed".to_string(),
                subject: "batch of 32".to_string(),
                ms: 940,
                left: 0,
            },
            EventKind::JobFailed {
                job: "summarize".to_string(),
                subject: "el:git:example/repo/src/a.rs::f".to_string(),
                error: "provider refused: 429".to_string(),
                attempts: 3,
                terminal: true,
            },
            EventKind::Queue {
                rows: vec![QueueDepth {
                    kind: "embed".to_string(),
                    state: "pending".to_string(),
                    count: 44,
                }],
            },
            EventKind::ScanProgress {
                root: "git:github.com/AI-Substrate/flowspace3".to_string(),
                root_path: "/srv/api".to_string(),
                files_seen: 1200,
                enqueued: 900,
                current: Some("crates/cli/src/main.rs".to_string()),
            },
            EventKind::RootChanged {
                change: "added".to_string(),
                root: "git:github.com/AI-Substrate/flowspace3".to_string(),
                root_path: "/srv/api".to_string(),
                files: 456,
            },
            EventKind::Heartbeat { seq: 7 },
        ];

        for kind in kinds {
            let event = Event::new("2026-08-28T03:11:05.001Z", kind.clone());
            let line = serde_json::to_string(&event).expect("an event serialises");
            let back: Event = serde_json::from_str(&line).expect("an event deserialises");
            assert_eq!(back, event, "line was {line}");
            assert_eq!(back.v, STREAM_VERSION);
        }
    }

    /// The guarantee that lets a shipped TUI meet a newer daemon: an unknown
    /// kind is DATA, not an error.
    #[test]
    fn a_kind_from_the_future_parses_instead_of_breaking_the_stream() {
        let line =
            r#"{"v":1,"at":"2026-08-28T03:11:05.001Z","kind":"reindex_started","plan":"full"}"#;
        let event: Event = serde_json::from_str(line).expect("an unknown kind still parses");

        assert_eq!(event.kind, EventKind::Unknown);
        assert_eq!(event.at, "2026-08-28T03:11:05.001Z");
    }

    /// A consumer attached to the wrong port finds out from the first line.
    #[test]
    fn a_hello_from_something_else_is_recognisably_not_ours() {
        let hello: Hello = serde_json::from_str(
            r#"{"stream":"fs3.events","v":1,"daemon":"0.4.0","heartbeat_ms":15000}"#,
        )
        .expect("our hello parses");

        assert_eq!(hello.stream, STREAM_NAME);
        assert!(serde_json::from_str::<Hello>(r#"{"hello":"world"}"#).is_err());
    }
}
