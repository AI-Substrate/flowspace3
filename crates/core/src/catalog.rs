//! The central error-code registry (workshop 004, decision D2).
//!
//! Every failure fs3 reports to a human or an agent carries a code from this
//! file, and every code carries the `fix` that resolves it. The registry is
//! **code, not documentation**: `docs/reference/error-codes.md` is generated
//! from [`ALL`], and a drift test fails when a code has no docs row — the same
//! encode-don't-document muscle as the architecture check.
//!
//! # Why a `fix` is mandatory
//!
//! Workshop 004's actionable-error doctrine: an error that only restates what
//! went wrong makes the reader do the diagnosis a second time. `fix` says what
//! to *do* — a command to run, a config line to write, or the one verb that
//! knows more (`flowspace3 doctor`). The field being **required by the type**
//! is what makes the doctrine stick; a reviewer never has to notice it is
//! missing, because a `Code` cannot be constructed without one.
//!
//! # Naming
//!
//! `FS3-E-<AREA>-<PROBLEM>`, SCREAMING-KEBAB. The areas are closed
//! ([`Area::ALL`]) so the namespace cannot sprawl, and a retired code is never
//! reused: a code in a log line from a year ago must still mean what it meant.
//!
//! # Adding one
//!
//! Add the `const`, add it to [`ALL`], and regenerate the docs page
//! (`cargo test -p fs3-core error_codes` names the command). Three edits in one
//! file, and the test refuses to let you forget the third.

use std::fmt;

/// The area of the system a code belongs to — the `<AREA>` segment.
///
/// Closed on purpose. A new area is a deliberate widening of the namespace,
/// which is a different decision from adding a code inside an existing one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Area {
    /// Configuration files, the environment layer, and the provider registry.
    Config,
    /// The Postgres + pgvector store: reachability, schema, queries.
    Store,
    /// Reading git: identity, snapshots, blob ids.
    Git,
    /// Discovery and parsing a worktree.
    Scan,
    /// An [`crate::Embedder`] or [`crate::Summarizer`] implementation.
    Provider,
    /// The job backlog and the worker loop.
    Queue,
    /// The query surface.
    Query,
    /// The daemon process itself, and reaching it.
    Daemon,
    /// The caller asked for something that is not a valid request.
    Usage,
}

impl Area {
    /// Every area, in the order the generated docs page lists them.
    pub const ALL: &'static [Area] = &[
        Area::Config,
        Area::Store,
        Area::Git,
        Area::Scan,
        Area::Provider,
        Area::Queue,
        Area::Query,
        Area::Daemon,
        Area::Usage,
    ];

    /// The `<AREA>` segment as it appears in a code.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Area::Config => "CONFIG",
            Area::Store => "STORE",
            Area::Git => "GIT",
            Area::Scan => "SCAN",
            Area::Provider => "PROVIDER",
            Area::Queue => "QUEUE",
            Area::Query => "QUERY",
            Area::Daemon => "DAEMON",
            Area::Usage => "USAGE",
        }
    }
}

impl fmt::Display for Area {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One registry entry: the code, what it means, and what to do about it.
///
/// Constructed only as a `const` in this file. That is the whole enforcement
/// mechanism behind "no ad-hoc error strings": a caller cannot invent a code at
/// a call site, because the fields are private and the constructor is not
/// public.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Code {
    code: &'static str,
    area: Area,
    summary: &'static str,
    fix: &'static str,
    retryable: bool,
}

impl Code {
    /// Private on purpose — see the type's own note.
    const fn new(
        code: &'static str,
        area: Area,
        summary: &'static str,
        fix: &'static str,
        retryable: bool,
    ) -> Self {
        Code {
            code,
            area,
            summary,
            fix,
            retryable,
        }
    }

    /// The wire spelling: `FS3-E-STORE-UNAVAILABLE`.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        self.code
    }

    /// Which part of the system this belongs to.
    #[must_use]
    pub const fn area(&self) -> Area {
        self.area
    }

    /// One line saying what this code means, for the generated docs page.
    #[must_use]
    pub const fn summary(&self) -> &'static str {
        self.summary
    }

    /// The default `fix` — what to DO, not what went wrong again.
    ///
    /// Callers with more context may replace it (a path, a variable name), but
    /// never remove it: [`crate::envelope::Failure`] requires one.
    #[must_use]
    pub const fn fix(&self) -> &'static str {
        self.fix
    }

    /// Whether repeating the same request could succeed without a change.
    ///
    /// Workshop 004 D5. The daemon's own job runner reads this to decide
    /// between re-queueing with backoff and failing the row terminally, so it
    /// is a real control signal, not advice for the reader.
    #[must_use]
    pub const fn retryable(&self) -> bool {
        self.retryable
    }

    /// The HTTP status a daemon endpoint answers with (workshop 004 D4).
    ///
    /// Mechanical, from the code's own spelling, so an endpoint author makes
    /// zero judgment calls: `*-INVALID*` is the caller's fault (400), a
    /// `*-NOT-FOUND` is missing (404), an `*-UNAVAILABLE` is a dependency that
    /// may come back (503), and anything else is ours (500).
    #[must_use]
    pub fn http_status(&self) -> u16 {
        if self.code.ends_with("-NOT-FOUND") {
            404
        } else if self.code.ends_with("-UNAVAILABLE") {
            503
        } else if self.code.contains("-INVALID") {
            400
        } else {
            500
        }
    }
}

impl fmt::Display for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code)
    }
}

/// The configuration parsed but does not describe a usable system.
pub const CONFIG_INVALID: Code = Code::new(
    "FS3-E-CONFIG-INVALID",
    Area::Config,
    "config.toml or an FS3_* override names a key, value or provider that cannot work.",
    "run `flowspace3 config show` to see the effective values and the layer each came from, then \
     correct the field the message names.",
    false,
);

/// A provider instance was selected whose credentials are not present.
pub const CONFIG_PROVIDER_UNKNOWN: Code = Code::new(
    "FS3-E-CONFIG-PROVIDER-UNKNOWN",
    Area::Config,
    "A port or repo selected a provider instance that is not in the registry.",
    "add the instance to config.toml (`[providers.<name>]` with a `kind`), or point the selection \
     at one that exists — `flowspace3 config show` lists the configured names.",
    false,
);

/// Postgres is not reachable at all.
pub const STORE_UNAVAILABLE: Code = Code::new(
    "FS3-E-STORE-UNAVAILABLE",
    Area::Store,
    "The Postgres + pgvector store did not answer.",
    "if the stack is not running: `docker compose up -d` — then re-run. `flowspace3 doctor` \
     diagnoses further.",
    true,
);

/// The database named in `database.url` does not exist yet.
pub const STORE_DATABASE_MISSING: Code = Code::new(
    "FS3-E-STORE-DATABASE-MISSING",
    Area::Store,
    "The server is up but the configured database has never been created.",
    "run `flowspace3 doctor` — it creates the database and applies every migration.",
    false,
);

/// The database exists but is behind the binary's embedded migrations.
pub const STORE_SCHEMA_STALE: Code = Code::new(
    "FS3-E-STORE-SCHEMA-STALE",
    Area::Store,
    "The database schema is older than the migrations embedded in this binary.",
    "run `flowspace3 doctor` — it applies the missing migrations and reports the result.",
    false,
);

/// A statement failed for a reason that is not the caller's to fix.
pub const STORE_QUERY_FAILED: Code = Code::new(
    "FS3-E-STORE-QUERY-FAILED",
    Area::Store,
    "A store statement failed.",
    "re-run once; if it repeats, `flowspace3 doctor` reports the store's state and the daemon log \
     carries the statement that failed.",
    true,
);

/// The path given is not inside a git worktree.
pub const GIT_NOT_A_WORKTREE: Code = Code::new(
    "FS3-E-GIT-NOT-A-WORKTREE",
    Area::Git,
    "The path is not inside a git worktree (a bare repository has nothing on disk to index).",
    "point the command at a checkout — the directory containing the files you want indexed.",
    false,
);

/// A path handed to `add`/`scan` is not on disk.
pub const SCAN_ROOT_NOT_FOUND: Code = Code::new(
    "FS3-E-SCAN-ROOT-NOT-FOUND",
    Area::Scan,
    "The root path does not exist, or is not a directory.",
    "check the path — `flowspace3 add <path>` takes an existing directory, and a relative path is \
     resolved against the daemon's working directory, so an absolute path is safer.",
    false,
);

/// A root was asked for that has never been registered.
pub const SCAN_ROOT_NOT_REGISTERED: Code = Code::new(
    "FS3-E-SCAN-ROOT-NOT-REGISTERED",
    Area::Scan,
    "The worktree is not registered, so there is nothing to re-scan.",
    "run `flowspace3 add <path>` first — `flowspace3 status` lists the roots that are registered.",
    false,
);

/// Discovery could not walk the root.
pub const SCAN_DISCOVERY_FAILED: Code = Code::new(
    "FS3-E-SCAN-DISCOVERY-FAILED",
    Area::Scan,
    "The discovery walk could not start.",
    "check the `[scan]` section: a `force_include` or `exclude` glob that does not compile stops \
     the walk before it begins.",
    false,
);

/// A file could not be parsed at all.
pub const SCAN_UNPARSEABLE: Code = Code::new(
    "FS3-E-SCAN-UNPARSEABLE",
    Area::Scan,
    "tree-sitter could not produce a tree for a file it has a grammar for.",
    "this is a defect, not a configuration problem — report the path and language; the file is \
     skipped and the rest of the scan continues.",
    false,
);

/// An embedder or summarizer failed.
pub const PROVIDER_FAILED: Code = Code::new(
    "FS3-E-PROVIDER-FAILED",
    Area::Provider,
    "The configured embedder or summarizer refused the call.",
    "check credentials and deployment names for the selected instance (`flowspace3 config show`); \
     `[providers.<name>] kind = \"fake\"` runs the whole stack offline while you fix it.",
    true,
);

/// A provider returned vectors of a width the store cannot hold.
pub const PROVIDER_DIMENSIONS: Code = Code::new(
    "FS3-E-PROVIDER-DIMENSIONS",
    Area::Provider,
    "The embedder returned vectors of a width no embeddings table holds.",
    "select a model whose width matches the configured table, or add an `embeddings_<width>` \
     migration for the new model before selecting it.",
    false,
);

/// A job exhausted its attempts.
pub const QUEUE_JOB_FAILED: Code = Code::new(
    "FS3-E-QUEUE-JOB-FAILED",
    Area::Queue,
    "A job failed every attempt and is now terminal.",
    "`flowspace3 status` reports failed jobs with their last error; fix the cause and re-run \
     `flowspace3 scan <path>` to re-enqueue the work.",
    false,
);

/// The query itself is not answerable as asked.
pub const QUERY_INVALID: Code = Code::new(
    "FS3-E-QUERY-INVALID",
    Area::Query,
    "The search request is not valid — an empty query, or a filter outside its range.",
    "check the flag the message names; `flowspace3 search --help` lists the accepted values.",
    false,
);

/// Nothing has been indexed for the selected model, so search cannot answer.
pub const QUERY_NO_INDEX: Code = Code::new(
    "FS3-E-QUERY-NO-INDEX",
    Area::Query,
    "No embeddings exist for the active model, so a semantic search has nothing to rank.",
    "run `flowspace3 add <path>` and wait for `flowspace3 status` to report an empty queue, then \
     search again.",
    false,
);

/// The daemon is not answering.
pub const DAEMON_UNAVAILABLE: Code = Code::new(
    "FS3-E-DAEMON-UNAVAILABLE",
    Area::Daemon,
    "The fs3 daemon did not answer on its configured URL.",
    "start it with `flowspace3 daemon &`, or run `flowspace3 doctor` to diagnose the stack. \
     Doctor reports the daemon but never starts one — a diagnostic command must not leave a \
     process running that you did not ask for.",
    true,
);

/// The command line itself is wrong.
pub const USAGE_INVALID: Code = Code::new(
    "FS3-E-USAGE-INVALID",
    Area::Usage,
    "The command was called with arguments it cannot act on.",
    "run the command with `--help`; the CLI exits 2 for usage problems, 1 for real failures.",
    false,
);

/// `docs get` was asked for a topic that is not bundled.
pub const USAGE_TOPIC_NOT_FOUND: Code = Code::new(
    "FS3-E-USAGE-TOPIC-NOT-FOUND",
    Area::Usage,
    "The requested documentation topic is not bundled in this binary.",
    "run `flowspace3 docs list` to see the topics this binary carries; the set is fixed at build \
     time, so a topic that is not listed does not exist in this version.",
    false,
);

/// Every registered code, in docs order.
///
/// The generated docs page and the drift test both read this, so a code that is
/// not here does not exist as far as fs3 is concerned.
pub const ALL: &[Code] = &[
    CONFIG_INVALID,
    CONFIG_PROVIDER_UNKNOWN,
    STORE_UNAVAILABLE,
    STORE_DATABASE_MISSING,
    STORE_SCHEMA_STALE,
    STORE_QUERY_FAILED,
    GIT_NOT_A_WORKTREE,
    SCAN_ROOT_NOT_FOUND,
    SCAN_ROOT_NOT_REGISTERED,
    SCAN_DISCOVERY_FAILED,
    SCAN_UNPARSEABLE,
    PROVIDER_FAILED,
    PROVIDER_DIMENSIONS,
    QUEUE_JOB_FAILED,
    QUERY_INVALID,
    QUERY_NO_INDEX,
    DAEMON_UNAVAILABLE,
    USAGE_INVALID,
    USAGE_TOPIC_NOT_FOUND,
];

/// Look up a code by its wire spelling — how a CLI turns a daemon's JSON back
/// into a registry entry.
#[must_use]
pub fn find(code: &str) -> Option<&'static Code> {
    ALL.iter().find(|entry| entry.code == code)
}

/// Render the whole registry as the markdown of `docs/reference/error-codes.md`.
///
/// Generated rather than written (workshop 004 D2): the page cannot drift from
/// the catalog because nobody edits it. The drift test compares this output to
/// the committed file byte for byte.
#[must_use]
pub fn markdown() -> String {
    let mut out = String::with_capacity(8 * 1024);
    out.push_str(
        "<!-- GENERATED from `fs3_core::catalog` — do not hand-edit.\n     Regenerate with \
         `FS3_UPDATE_DOCS=1 cargo test -p fs3-core error_codes`. -->\n# fs3 error codes\n\nEvery \
         failure fs3 reports carries one of these codes and the `fix` beside it. The\nregistry is \
         `crates/core/src/catalog.rs`; this page is emitted from it.\n\n`retryable` means \
         repeating the same request could succeed without a change — the\ndaemon's job runner \
         reads it to choose between re-queueing and failing a row.\n\n`status` is the HTTP status \
         a daemon endpoint answers with, derived mechanically\nfrom the code's own spelling \
         (workshop 004 D4).\n",
    );

    for area in Area::ALL {
        let codes: Vec<&Code> = ALL.iter().filter(|entry| entry.area == *area).collect();
        if codes.is_empty() {
            continue;
        }
        out.push_str(&format!("\n## {area}\n\n"));
        for entry in codes {
            out.push_str(&format!("### `{}`\n\n", entry.code));
            out.push_str(&format!("{}\n\n", entry.summary));
            out.push_str(&format!("**Fix**: {}\n\n", entry.fix));
            out.push_str(&format!(
                "| retryable | status |\n| --- | --- |\n| {} | {} |\n",
                entry.retryable,
                entry.http_status()
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The naming rule, enforced rather than remembered: a code that does not
    /// start `FS3-E-<AREA>-` is unfindable by the greps people actually run.
    #[test]
    fn every_code_is_spelled_the_way_the_registry_promises() {
        for entry in ALL {
            let prefix = format!("FS3-E-{}-", entry.area);
            assert!(
                entry.code.starts_with(&prefix),
                "{} must start with {prefix}",
                entry.code
            );
            assert!(
                entry
                    .code
                    .bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'-'),
                "{} must be SCREAMING-KEBAB (the digit is the 3 in FS3)",
                entry.code
            );
        }
    }

    /// A code is a stable identifier forever, so two entries sharing one
    /// spelling would make a log line ambiguous a year from now.
    #[test]
    fn codes_are_unique() {
        let mut seen: Vec<&str> = ALL.iter().map(|entry| entry.code).collect();
        seen.sort_unstable();
        let before = seen.len();
        seen.dedup();
        assert_eq!(before, seen.len(), "duplicate error code in the registry");
    }

    /// D3: the `fix` field being mandatory is what makes the doctrine stick.
    /// An empty one would satisfy the type and defeat the point.
    #[test]
    fn every_code_carries_a_real_fix_and_summary() {
        for entry in ALL {
            assert!(
                entry.fix.len() > 20,
                "{}'s fix must say what to DO, got {:?}",
                entry.code,
                entry.fix
            );
            assert!(
                entry.summary.len() > 20,
                "{} needs a summary for the docs page",
                entry.code
            );
        }
    }

    #[test]
    fn http_status_is_derived_from_the_code_class() {
        assert_eq!(CONFIG_INVALID.http_status(), 400);
        assert_eq!(QUERY_INVALID.http_status(), 400);
        assert_eq!(SCAN_ROOT_NOT_FOUND.http_status(), 404);
        assert_eq!(STORE_UNAVAILABLE.http_status(), 503);
        assert_eq!(DAEMON_UNAVAILABLE.http_status(), 503);
        // Not the caller's fault and not a dependency being down: ours.
        assert_eq!(STORE_SCHEMA_STALE.http_status(), 500);
        assert_eq!(PROVIDER_FAILED.http_status(), 500);
    }

    #[test]
    fn a_code_round_trips_through_its_wire_spelling() {
        assert_eq!(find(STORE_SCHEMA_STALE.as_str()), Some(&STORE_SCHEMA_STALE));
        assert_eq!(find("FS3-E-NOT-A-REAL-CODE"), None);
    }
}
