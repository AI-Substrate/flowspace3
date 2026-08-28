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
//! (`cargo test -p fs3-core --test error_codes` names the command). Three edits in one
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
    /// Keeping the installed binary current (PRD req 54).
    Update,
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
        Area::Update,
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
            Area::Update => "UPDATE",
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
    /// zero judgment calls: `*-UNAUTHORIZED` is missing or stale authentication
    /// (401), `*-INVALID*` is the caller's fault (400), a `*-NOT-FOUND` is
    /// missing (404), a `*-NOT-IMPLEMENTED` is a feature this build does not
    /// have (501), an `*-UNAVAILABLE` is a dependency that may come back (503),
    /// and anything else is ours (500).
    ///
    /// Authentication is checked before route dispatch, so its status must be
    /// derivable from the code just like every endpoint failure.
    ///
    /// The 501 arm exists because the alternative was worse than untidy: a
    /// valid `conv:` address answered with 500 tells a caller that fs3 broke,
    /// when what actually happened is that the feature is not built yet. One
    /// arm keeps the mapping mechanical AND keeps the answer honest.
    #[must_use]
    pub fn http_status(&self) -> u16 {
        if self.code.ends_with("-UNAUTHORIZED") {
            401
        } else if self.code.ends_with("-NOT-FOUND") {
            404
        } else if self.code.ends_with("-NOT-IMPLEMENTED") {
            501
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

/// A provider asked us to slow down.
///
/// Distinct from [`PROVIDER_FAILED`] because the RESPONSE is different: a
/// failure asks the worker to retry on its own schedule and count the attempt,
/// while congestion asks it to wait the service's own interval and count
/// nothing. Reporting a 429 as a generic provider failure would spend the
/// job's attempts on a provider that is working perfectly and simply busy.
pub const PROVIDER_RATE_LIMITED: Code = Code::new(
    "FS3-E-PROVIDER-RATE-LIMITED",
    Area::Provider,
    "The provider is rate limiting us.",
    "nothing, usually — the job is parked and retried on the service's own \
     schedule. If it persists, lower `worker_concurrency` or raise the \
     deployment's quota; `flowspace3 doctor` names the active instance.",
    true,
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

/// The agent port is wired to a provider that cannot answer anything.
///
/// Distinct from [`PROVIDER_FAILED`] because nothing FAILED: the stack is
/// healthy and the port is unusable, which is a configuration answer rather
/// than a runtime one. It exists because `kind = "fake"` is a legal keyless
/// production value — for the embedder and summarizer that is genuinely
/// useful, since a fake embedder emits real vectors and search works offline.
/// A fake CHAT model has no honest output, so the verb must refuse rather
/// than publish a placeholder on an `ok: true` envelope where a machine
/// consumer would bank it as a finding.
pub const PROVIDER_CANNOT_ANSWER: Code = Code::new(
    "FS3-E-PROVIDER-CANNOT-ANSWER",
    Area::Provider,
    "The agent port is configured with a provider that cannot answer questions.",
    "point `[agent] active` at a real chat deployment (`flowspace3 config show` names the \
     current one, `flowspace3 docs get providers` sets one up). The offline `fake` runs the \
     rest of the stack without keys, but it cannot answer a question.",
    false,
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

/// The address is well formed but nothing in the index answers to it.
pub const QUERY_NOT_FOUND: Code = Code::new(
    "FS3-E-QUERY-NOT-FOUND",
    Area::Query,
    "No repository, file or element in the index answers to the address that was asked for.",
    "check the address against a search hit — `flowspace3 search \"<question>\"` prints the \
     address of everything it returns, and `flowspace3 tree <repo-or-path>` lists what is \
     actually indexed under a path.",
    false,
);

/// The address cannot be read as an address at all.
pub const QUERY_INVALID_ADDRESS: Code = Code::new(
    "FS3-E-QUERY-INVALID-ADDRESS",
    Area::Query,
    "The address does not parse: it must be `el:<repo>/<path>::<name>` or `conv:<guid>`.",
    "copy the `address` field from a search hit rather than composing one by hand; \
     `flowspace3 search \"<question>\"` prints it for every result.",
    false,
);

/// The address is real and matches more than one thing.
///
/// Not a defect: `struct Rect` and `impl Rect` are two elements at ONE address
/// by design (workshop 002 — `(address, span_start)` is what identifies an
/// element), and a path that exists in two repositories is two files.
pub const QUERY_INVALID_AMBIGUOUS: Code = Code::new(
    "FS3-E-QUERY-INVALID-AMBIGUOUS",
    Area::Query,
    "The address matches more than one element or repository, so there is no single answer.",
    "narrow it: `--span <line>` picks one of several elements sharing an address (the \
     candidates are listed in `details`), and `--repo <identity>` picks one repository.",
    false,
);

/// The request is understood and names something this build cannot answer yet.
///
/// Deliberately kept after conversations landed and stopped being its only
/// user. It is the code — and the 501 arm in [`Code::http_status`] — that any
/// future surface reaches for when an address or a verb is real in the design
/// and absent from the binary: the MCP tools, the corpora after conversations.
/// Retiring it would mean the next such case either invents a code or lies
/// with a 500, and a retired code can never be reused.
pub const QUERY_NOT_IMPLEMENTED: Code = Code::new(
    "FS3-E-QUERY-NOT-IMPLEMENTED",
    Area::Query,
    "The request is valid but names something this build does not implement yet.",
    "nothing to fix in your request — the message names what is missing. \
     `flowspace3 docs list` describes what this version does answer, and \
     `flowspace3 doctor upgrade` installs a newer one.",
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

/// The daemon rejected a request that carried no current boot key.
pub const DAEMON_UNAUTHORIZED: Code = Code::new(
    "FS3-E-DAEMON-UNAUTHORIZED",
    Area::Daemon,
    "The request did not present the bearer key generated by the running fs3 daemon.",
    "read daemon.key from the resolved fs3 config directory and send it as `Authorization: \
     Bearer <key>`; if the file is missing or stale, restart the daemon so it republishes it.",
    false,
);

/// The release could not be read.
pub const UPDATE_UNREACHABLE: Code = Code::new(
    "FS3-E-UPDATE-UNREACHABLE",
    Area::Update,
    "The published release list could not be read, so there is nothing to compare against.",
    "check network access to https://github.com/AI-Substrate/flowspace3/releases and try \
     again; the daemon retries on its own schedule, so a transient outage needs no action.",
    true,
);

/// This process cannot work out which file to replace.
pub const UPDATE_NO_INSTALL_PATH: Code = Code::new(
    "FS3-E-UPDATE-NO-INSTALL-PATH",
    Area::Update,
    "This process cannot resolve its own executable, so there is no binary to replace.",
    "reinstall instead: `curl -fsSL \
     https://raw.githubusercontent.com/AI-Substrate/flowspace3/main/install.sh | sh`.",
    false,
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
    PROVIDER_CANNOT_ANSWER,
    PROVIDER_FAILED,
    PROVIDER_RATE_LIMITED,
    PROVIDER_DIMENSIONS,
    QUEUE_JOB_FAILED,
    QUERY_INVALID,
    QUERY_NO_INDEX,
    QUERY_NOT_FOUND,
    QUERY_INVALID_ADDRESS,
    QUERY_INVALID_AMBIGUOUS,
    QUERY_NOT_IMPLEMENTED,
    DAEMON_UNAVAILABLE,
    DAEMON_UNAUTHORIZED,
    UPDATE_UNREACHABLE,
    UPDATE_NO_INSTALL_PATH,
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
         `FS3_UPDATE_DOCS=1 cargo test -p fs3-core --test error_codes`. -->\n# fs3 error codes\n\nEvery \
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
        assert_eq!(DAEMON_UNAUTHORIZED.http_status(), 401);
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
